mod ui;

fn main() -> eframe::Result {
    let icon = image::load_from_memory(include_bytes!("../assets/zarg-icon.png"))
        .expect("embedded app icon must be a valid PNG")
        .into_rgba8();
    let (width, height) = icon.dimensions();
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 680.0])
            .with_min_inner_size([720.0, 460.0])
            .with_app_id("zarg")
            .with_icon(eframe::egui::IconData {
                rgba: icon.into_raw(),
                width,
                height,
            }),
        ..Default::default()
    };
    eframe::run_native(
        "ZAR Archive Manager",
        options,
        Box::new(|_creation_context| Ok(Box::<ui::ZargApp>::default())),
    )
}
