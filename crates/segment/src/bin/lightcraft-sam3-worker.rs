fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let dir = args.next().ok_or("usage: lightcraft-sam3-worker MODEL_DIR [127.0.0.1:8793]")?;
    let address = args.next().unwrap_or_else(|| "127.0.0.1:8793".into()).parse()?;
    let device = candle_core::Device::new_metal(0)?;
    lightcraft_segment::remote::serve(address, std::path::Path::new(&dir), device)?;
    Ok(())
}
