// Box passes (clamped edges) over an image of NC interleaved channels; three of each approximate a
// Gaussian (`lightcraft_raster::blur::gaussian`). Each thread slides a running sum over CH pixels.
// P: w, h, nc, r, ch. Bindings: src, dst.

// Horizontal: one thread per CH consecutive pixels of a row.
@compute @workgroup_size(64, 4)
fn box_h(@builtin(global_invocation_id) g: vec3<u32>) {
    let w = pu(0u);
    let h = pu(1u);
    let nc = pu(2u);
    let r = i32(pu(3u));
    let ch = pu(4u);
    let y = g.y;
    let x0 = g.x * ch;
    if (y >= h || x0 >= w) {
        return;
    }
    let last = i32(w) - 1;
    let inv = 1.0 / f32(2 * r + 1);
    let row = y * w;
    let x1 = min(x0 + ch, w);
    for (var c = 0u; c < nc; c++) {
        var acc = 0.0;
        for (var k = -r; k <= r; k++) {
            acc += src[(row + u32(clamp(i32(x0) + k, 0, last))) * nc + c];
        }
        for (var x = x0; x < x1; x++) {
            dst[(row + x) * nc + c] = acc * inv;
            acc = acc + src[(row + u32(min(i32(x) + r + 1, last))) * nc + c] - src[(row + u32(max(i32(x) - r, 0))) * nc + c];
        }
    }
}

// Vertical: one thread per CH consecutive rows of a column (the CPU re-primes every BAND rows the
// same way).
@compute @workgroup_size(64, 4)
fn box_v(@builtin(global_invocation_id) g: vec3<u32>) {
    let w = pu(0u);
    let h = pu(1u);
    let nc = pu(2u);
    let r = i32(pu(3u));
    let ch = pu(4u);
    let x = g.x;
    let y0 = g.y * ch;
    if (x >= w || y0 >= h) {
        return;
    }
    let last = i32(h) - 1;
    let inv = 1.0 / f32(2 * r + 1);
    let y1 = min(y0 + ch, h);
    for (var c = 0u; c < nc; c++) {
        var acc = 0.0;
        for (var k = -r; k <= r; k++) {
            acc += src[(u32(clamp(i32(y0) + k, 0, last)) * w + x) * nc + c];
        }
        for (var y = y0; y < y1; y++) {
            dst[(y * w + x) * nc + c] = acc * inv;
            acc = acc + src[(u32(min(i32(y) + r + 1, last)) * w + x) * nc + c] - src[(u32(max(i32(y) - r, 0)) * w + x) * nc + c];
        }
    }
}
