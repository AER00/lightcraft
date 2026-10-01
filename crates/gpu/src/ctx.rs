//! The wgpu device, the compiled kernels, buffers, dispatch and readback.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use wgpu::util::DeviceExt;

/// A kernel module: WGSL source, its storage bindings (name, writable, element type) after the
/// parameter block `P` (binding 0), and its entry points.
struct Module {
    src: &'static str,
    bindings: &'static [(&'static str, bool, &'static str)],
    entries: &'static [&'static str],
}

const MODULES: &[Module] = &[
    Module {
        src: include_str!("wgsl/finish.wgsl"),
        bindings: &[
            ("img", false, "f32"),
            ("log_l", false, "f32"),
            ("base_p", false, "f32"),
            ("clar", false, "f32"),
            ("tex", false, "f32"),
            ("dark", false, "f32"),
            ("masks", false, "f32"),
            ("aux", false, "f32"),
            ("out", true, "u32"),
        ],
        entries: &["main"],
    },
    Module { src: include_str!("wgsl/blur.wgsl"), bindings: &[("src", false, "f32"), ("dst", true, "f32")], entries: &["box_h", "box_v"] },
    Module {
        src: include_str!("wgsl/resize.wgsl"),
        bindings: &[("src", false, "f32"), ("table", false, "u32"), ("dst", true, "f32")],
        entries: &["resize_h", "resize_v"],
    },
    Module {
        src: include_str!("wgsl/map.wgsl"),
        bindings: &[("a", false, "f32"), ("b", false, "f32"), ("c", false, "f32"), ("dst", true, "f32")],
        entries: &["log_lum_k", "dark_k", "guided_pre", "guided_ab", "guided_apply", "wb_k", "nr_lum", "chroma_k", "nr_col", "subsample"],
    },
    Module {
        src: include_str!("wgsl/mask.wgsl"),
        bindings: &[("img", false, "f32"), ("log_l", false, "f32"), ("aux", false, "f32"), ("c", true, "f32"), ("alpha", true, "f32")],
        entries: &["shape", "combine", "finalize"],
    },
    Module {
        src: include_str!("wgsl/geom.wgsl"),
        bindings: &[("src", false, "f32"), ("dst", true, "f32")],
        entries: &["orient", "sample_affine", "sample_warp"],
    },
];

struct Kernel {
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    nbuf: usize,
}

/// A device buffer of `len` 32-bit values.
///
/// Dropped buffers are recycled: they first wait in a per-thread list (commands recorded on this
/// thread may still use them) and become reusable once this thread submits ([`Gpu::submitted`]),
/// as the queue then runs every earlier use before any later one. Reuse matters: allocating (and
/// zero-filling) a fresh 100–300 MB buffer per pass costs as much as the pass itself.
pub struct Buf {
    buf: Option<Tracked>,
    pub len: usize,
}

impl Buf {
    pub fn raw(&self) -> &wgpu::Buffer {
        &self.buf.as_ref().expect("live buffer").0
    }
}

impl Drop for Buf {
    fn drop(&mut self) {
        let Some(b) = self.buf.take() else { return };
        // Outside a render this thread has no commands recorded (only renders record), so the
        // buffer is reusable at once; without this, buffers dropped on threads that rarely
        // submit (the UI thread clearing a view's stages) would wait indefinitely.
        if !IN_RENDER.with(|c| c.get() > 0)
            && let Some(g) = crate::existing_device()
        {
            g.pool([b]);
            return;
        }
        RETIRED_BYTES.fetch_add(b.0.size(), Ordering::Relaxed);
        RETIRED.with(|r| r.borrow_mut().push(b));
    }
}

thread_local! {
    /// Renders running on this thread (they may hold recorded, unsubmitted commands).
    static IN_RENDER: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Marks a render on this thread; when it ends, the buffers it released join the pool (everything
/// it recorded has been submitted by then).
pub(crate) struct RenderScope<'a>(&'a Gpu);

impl<'a> RenderScope<'a> {
    pub fn new(g: &'a Gpu) -> RenderScope<'a> {
        IN_RENDER.with(|c| c.set(c.get() + 1));
        RenderScope(g)
    }
}

impl Drop for RenderScope<'_> {
    fn drop(&mut self) {
        let outer = IN_RENDER.with(|c| {
            c.set(c.get().saturating_sub(1));
            c.get() == 0
        });
        if outer {
            let retired = RETIRED.with(|r| std::mem::take(&mut *r.borrow_mut()));
            RETIRED_BYTES.fetch_sub(retired.iter().map(|b| b.0.size()).sum(), Ordering::Relaxed);
            self.0.pool(retired);
        }
    }
}

/// A device buffer, counted in [`ALLOCATED`] while it exists.
pub(crate) struct Tracked(wgpu::Buffer);

impl Tracked {
    fn new(b: wgpu::Buffer) -> Tracked {
        ALLOCATED.fetch_add(b.size(), Ordering::Relaxed);
        Tracked(b)
    }
}

impl Drop for Tracked {
    fn drop(&mut self) {
        ALLOCATED.fetch_sub(self.0.size(), Ordering::Relaxed);
    }
}

thread_local! {
    /// Buffers dropped on this thread since its last submit.
    static RETIRED: std::cell::RefCell<Vec<Tracked>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Bytes of device buffers that exist (in use, retired or pooled).
static ALLOCATED: AtomicU64 = AtomicU64::new(0);
/// Bytes waiting in per-thread retired lists.
static RETIRED_BYTES: AtomicU64 = AtomicU64::new(0);
/// Bytes in the free pool.
static POOLED: AtomicU64 = AtomicU64::new(0);
/// Most bytes kept in the free pool (see [`crate::set_pool_limit`]).
pub(crate) static POOL_LIMIT: AtomicU64 = AtomicU64::new(DEFAULT_POOL_BYTES);

/// Default for the most bytes kept in the free pool (apps derive it from their memory budget).
pub(crate) const DEFAULT_POOL_BYTES: u64 = 256 << 20;

/// Device buffers held by the renderer (bytes).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GpuMemory {
    /// Every buffer that exists (in use + retired + pooled).
    pub allocated: u64,
    /// Of which recycled buffers waiting in the free pool.
    pub pooled: u64,
    /// Of which buffers dropped since their thread's last submit (pooled at its next submit).
    pub retired: u64,
}

pub(crate) fn memory() -> GpuMemory {
    GpuMemory { allocated: ALLOCATED.load(Ordering::Relaxed), pooled: POOLED.load(Ordering::Relaxed), retired: RETIRED_BYTES.load(Ordering::Relaxed) }
}

pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub info: wgpu::AdapterInfo,
    kernels: HashMap<&'static str, Kernel>,
    dummy: wgpu::Buffer,
    /// Recycled buffers (see [`Buf`]).
    free: std::sync::Mutex<Vec<Tracked>>,
    /// Largest storage buffer the device accepts (bytes).
    pub max_buffer: u64,
}

/// WGSL constants shared by every module (generated from the CPU pipeline's values).
fn constants() -> String {
    use lightcraft_pipeline::finish::{GRAIN_HASH, MASK_TERMS, SRGB_LUT_N};
    use lightcraft_pipeline::tone::{LUT_MAX_EV, LUT_MIN_EV, LUT_N};
    let mut s = String::new();
    let [to_lms, from_lms, to_lab, from_lab] = lightcraft_color::perceptual::oklab_matrices();
    let to_srgb = lightcraft_color::REC2020.to_space(&lightcraft_color::SRGB).to_f32();
    for (name, m) in [("OK_TO_LMS", to_lms), ("OK_FROM_LMS", from_lms), ("OK_TO_LAB", to_lab), ("OK_FROM_LAB", from_lab), ("TO_SRGB", to_srgb)] {
        let rows: Vec<String> = m.iter().map(|r| format!("vec3<f32>({:?}, {:?}, {:?})", r[0], r[1], r[2])).collect();
        s += &format!("const {name} = array<vec3<f32>, 3>({});\n", rows.join(", "));
    }
    s += &format!("const TONE_MIN_EV: f32 = {LUT_MIN_EV:?};\nconst TONE_MAX_EV: f32 = {LUT_MAX_EV:?};\nconst TONE_N: u32 = {LUT_N}u;\n");
    s += &format!("const TONE_MIN_GAIN: f32 = {:?};\n", 2f32.powf(LUT_MIN_EV));
    let b = lightcraft_pipeline::geometry::BLANK;
    s += &format!("const BLANK_R: f32 = {:?};\nconst BLANK_G: f32 = {:?};\nconst BLANK_B: f32 = {:?};\n", b[0], b[1], b[2]);
    s += &format!("const SRGB_N: u32 = {SRGB_LUT_N}u;\nconst CURVE_N: u32 = {}u;\nconst MASK_TERMS: u32 = {MASK_TERMS}u;\n", crate::params::CURVE_N);
    for (i, h) in GRAIN_HASH.iter().enumerate() {
        s += &format!("const GRAIN_H{i}: u32 = {h}u;\n");
    }
    for (name, idx) in crate::params::finish_fields() {
        s += &format!("const F_{name}: u32 = {idx}u;\n");
    }
    s
}

fn module_source(m: &Module, consts: &str) -> String {
    let mut s = String::from(consts);
    s += "@group(0) @binding(0) var<storage, read> P: array<u32>;\n";
    for (i, (name, write, ty)) in m.bindings.iter().enumerate() {
        let access = if *write { "read_write" } else { "read" };
        s += &format!("@group(0) @binding({}) var<storage, {access}> {name}: array<{ty}>;\n", i + 1);
    }
    s += include_str!("wgsl/common.wgsl");
    s += m.src;
    s
}

impl Gpu {
    /// Create a device on the best available adapter (no software fallback), or `None`.
    pub fn new() -> Option<Gpu> {
        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
        desc.backends = wgpu::Backends::PRIMARY;
        let instance = wgpu::Instance::new(desc);
        let adapter = pollster::block_on(
            instance.request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, ..Default::default() }),
        )
        .ok()?;
        let info = adapter.get_info();
        if info.device_type == wgpu::DeviceType::Cpu {
            log::info!("gpu: only a software adapter ({}), using the CPU pipeline", info.name);
            return None;
        }
        let limits = adapter.limits();
        if limits.max_storage_buffers_per_shader_stage < 10 {
            log::info!("gpu: {} has too few storage buffers per stage", info.name);
            return None;
        }
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("lightcraft"),
            required_limits: limits.clone(),
            ..Default::default()
        }))
        .ok()?;
        // Errors outside an error scope (e.g. out of memory during a render): log, and stop using
        // the GPU (the render in flight is redone on the CPU).
        device.on_uncaptured_error(std::sync::Arc::new(|e| {
            log::error!("gpu: {e}");
            crate::mark_broken();
        }));
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let consts = constants();
        let mut kernels = HashMap::new();
        for m in MODULES {
            let src = module_source(m, &consts);
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: None, source: wgpu::ShaderSource::Wgsl(src.into()) });
            let mut entries = vec![storage_entry(0, true)];
            entries.extend(m.bindings.iter().enumerate().map(|(i, (_, w, _))| storage_entry(i as u32 + 1, !w)));
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: None, entries: &entries });
            let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            });
            for e in m.entries {
                let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some(e),
                    layout: Some(&pl),
                    module: &module,
                    entry_point: Some(e),
                    compilation_options: Default::default(),
                    cache: None,
                });
                kernels.insert(*e, Kernel { pipeline, layout: layout.clone(), nbuf: m.bindings.len() });
            }
        }
        if let Some(e) = pollster::block_on(scope.pop()) {
            log::error!("gpu: kernels failed to build on {}: {e}", info.name);
            return None;
        }
        let dummy = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("dummy"),
            size: 16,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let max_buffer = limits.max_storage_buffer_binding_size.min(limits.max_buffer_size);
        log::info!("gpu: {} ({:?})", info.name, info.backend);
        Some(Gpu { device, queue, info, kernels, dummy, free: Default::default(), max_buffer })
    }

    /// A buffer of `len` 32-bit values with undefined contents (a recycled one when possible).
    pub fn buffer(&self, len: usize) -> Buf {
        let size = (len.max(1) * 4) as u64;
        let recycled = {
            let mut f = self.free.lock().unwrap_or_else(|e| e.into_inner());
            // the smallest pooled buffer that holds `size` with at most 25 % to spare: photos of
            // slightly different sizes reuse each other's buffers instead of piling up new ones
            let b = f
                .iter()
                .enumerate()
                .filter(|(_, b)| b.0.size() >= size && b.0.size() <= size + size / 4)
                .min_by_key(|(_, b)| b.0.size())
                .map(|(i, _)| i)
                .map(|i| f.swap_remove(i));
            if let Some(b) = &b {
                POOLED.fetch_sub(b.0.size(), Ordering::Relaxed);
            }
            b
        };
        let buf = recycled.unwrap_or_else(|| {
            Tracked::new(self.device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }))
        });
        Buf { buf: Some(buf), len }
    }

    /// Submit command buffers, then make the buffers this thread dropped before reusable.
    pub fn submit(&self, cmds: impl IntoIterator<Item = wgpu::CommandBuffer>) {
        self.queue.submit(cmds);
        let retired = RETIRED.with(|r| std::mem::take(&mut *r.borrow_mut()));
        RETIRED_BYTES.fetch_sub(retired.iter().map(|b| b.0.size()).sum(), Ordering::Relaxed);
        self.pool(retired);
    }

    /// Put reusable buffers into the free pool (trimmed to its limit).
    fn pool(&self, bufs: impl IntoIterator<Item = Tracked>) {
        let mut added = false;
        {
            let mut f = self.free.lock().unwrap_or_else(|e| e.into_inner());
            for b in bufs {
                POOLED.fetch_add(b.0.size(), Ordering::Relaxed);
                f.push(b);
                added = true;
            }
        }
        if added {
            self.trim(POOL_LIMIT.load(Ordering::Relaxed));
        }
    }

    /// Free pooled buffers (oldest first) until at most `keep` bytes stay pooled.
    pub fn trim(&self, keep: u64) {
        let mut f = self.free.lock().unwrap_or_else(|e| e.into_inner());
        let mut total: u64 = f.iter().map(|b| b.0.size()).sum();
        let mut dropped = Vec::new();
        while total > keep && !f.is_empty() {
            let b = f.remove(0);
            total -= b.0.size();
            POOLED.fetch_sub(b.0.size(), Ordering::Relaxed);
            dropped.push(b);
        }
        drop(f);
        if !dropped.is_empty() {
            drop(dropped);
            // let wgpu release the memory now rather than at its next maintenance
            let _ = self.device.poll(wgpu::PollType::Poll);
        }
    }

    /// Upload 32-bit values.
    pub fn upload<T: bytemuck::Pod>(&self, data: &[T]) -> Buf {
        let bytes: &[u8] = bytemuck::cast_slice(data);
        let len = bytes.len() / 4;
        if bytes.is_empty() {
            return self.buffer(1);
        }
        let buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
        });
        Buf { buf: Some(Tracked::new(buf)), len }
    }

    /// Whether a buffer of `len` 32-bit values fits the device limits.
    pub fn fits(&self, len: usize) -> bool {
        (len as u64) * 4 <= self.max_buffer
    }

    pub fn encoder(&self) -> wgpu::CommandEncoder {
        self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None })
    }

    /// Record kernel `name` with parameters `p` over buffers `bufs` (None = unused binding).
    pub fn run(&self, enc: &mut wgpu::CommandEncoder, name: &str, p: &[u32], bufs: &[Option<&Buf>], groups: [u32; 3]) {
        let k = self.kernels.get(name).unwrap_or_else(|| panic!("unknown kernel {name}"));
        assert_eq!(bufs.len(), k.nbuf, "{name}: binding count");
        let params = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(if p.is_empty() { &[0u32] } else { p }),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let mut entries = vec![wgpu::BindGroupEntry { binding: 0, resource: params.as_entire_binding() }];
        for (i, b) in bufs.iter().enumerate() {
            let buf = b.map(|b| b.raw()).unwrap_or(&self.dummy);
            entries.push(wgpu::BindGroupEntry { binding: i as u32 + 1, resource: buf.as_entire_binding() });
        }
        let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &k.layout, entries: &entries });
        let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some(name), timestamp_writes: None });
        pass.set_pipeline(&k.pipeline);
        pass.set_bind_group(0, &bg, &[]);
        pass.dispatch_workgroups(groups[0], groups[1], groups[2]);
    }

    /// Submit `enc`, then read back the first `len` 32-bit values of `src`.
    pub fn finish_and_read<T: bytemuck::Pod>(&self, mut enc: wgpu::CommandEncoder, src: &Buf, len: usize) -> Vec<T> {
        assert!(len <= src.len, "readback of {len} values from a buffer of {}", src.len);
        let size = (len * 4) as u64;
        let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: size.max(4),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        enc.copy_buffer_to_buffer(src.raw(), 0, &staging, 0, size);
        self.submit([enc.finish()]);
        let slice = staging.slice(..size);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        match rx.recv() {
            Ok(Ok(())) => {}
            other => panic!("gpu readback failed: {other:?}"),
        }
        let data = slice.get_mapped_range().expect("mapped");
        let out: Vec<T> = bytemuck::cast_slice(&data).to_vec();
        drop(data);
        staging.unmap();
        out
    }
}

fn storage_entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only }, has_dynamic_offset: false, min_binding_size: None },
        count: None,
    }
}

/// Workgroups covering a `w × h` grid with `wg`-sized groups.
pub fn groups2(w: usize, h: usize, wg: [usize; 2]) -> [u32; 3] {
    [w.div_ceil(wg[0]).max(1) as u32, h.div_ceil(wg[1]).max(1) as u32, 1]
}

/// Workgroups for a 1-D kernel of `n` threads (256 per group) on a 2-D grid (see `lin_index`).
pub fn groups1(n: usize) -> [u32; 3] {
    let g = n.div_ceil(256).max(1);
    let x = g.min(32768);
    [x as u32, g.div_ceil(x) as u32, 1]
}
