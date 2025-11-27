use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;

use eframe::egui;
use image::{ImageFormat, DynamicImage};
use rfd::FileDialog;

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([500.0, 400.0])
            .with_resizable(true),
        ..Default::default()
    };

    eframe::run_native(
        "Image Compressor",
        options,
        Box::new(|_cc| Ok(Box::new(ImageCompressorApp::default()))),
    )
}

struct CompressionTask {
    input_path: PathBuf,
    output_path: PathBuf,
    quality: u8,
}

struct ImageCompressorApp {
    input_path: Option<PathBuf>,
    output_path: Option<PathBuf>,
    quality: u8,
    status_message: String,
    compress_tx: Sender<CompressionTask>,
    result_rx: Receiver<Result<String, String>>,
    is_compressing: bool,
}

impl Default for ImageCompressorApp {
    fn default() -> Self {
        let (compress_tx, compress_rx) = channel::<CompressionTask>();
        let (result_tx, result_rx) = channel::<Result<String, String>>();

        // Spawn worker thread for compression
        thread::spawn(move || {
            while let Ok(task) = compress_rx.recv() {
                let result = perform_compression(task);
                let _ = result_tx.send(result);
            }
        });

        Self {
            input_path: None,
            output_path: None,
            quality: 80,
            status_message: "Ready".to_string(),
            compress_tx,
            result_rx,
            is_compressing: false,
        }
    }
}

impl eframe::App for ImageCompressorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check for compression results without blocking
        if let Ok(result) = self.result_rx.try_recv() {
            self.is_compressing = false;
            self.status_message = match result {
                Ok(msg) => msg,
                Err(err) => err,
            };
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Image Compressor");
            ui.add_space(10.0);

            // Input file
            ui.horizontal(|ui| {
                ui.label("Input file:");
                if ui.button("Browse…").clicked() {
                    if let Some(path) = FileDialog::new()
                        .add_filter("Images", &["png", "jpg", "jpeg", "webp"])
                        .pick_file()
                    {
                        self.input_path = Some(path);
                        self.status_message = "Input file selected".to_string();
                    }
                }
            });

            ui.label(
                self.input_path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "No file selected".to_string()),
            );
            ui.add_space(10.0);

            // Output file
            ui.horizontal(|ui| {
                ui.label("Output file:");
                if ui.button("Browse…").clicked() {
                    if let Some(path) = FileDialog::new()
                        .add_filter("JPEG", &["jpg", "jpeg"])
                        .add_filter("PNG", &["png"])
                        .add_filter("WebP", &["webp"])
                        .save_file()
                    {
                        self.output_path = Some(path);
                        self.status_message = "Output file selected".to_string();
                    }
                }
            });

            ui.label(
                self.output_path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "No file selected".to_string()),
            );
            ui.add_space(10.0);

            // Quality slider
            ui.separator();
            ui.add_space(10.0);

            ui.horizontal(|ui| {
                ui.label("Compression quality:");
                ui.label(format!("{}%", self.quality));
            });

            ui.add(
                egui::Slider::new(&mut self.quality, 1..=100)
                    .text("Quality")
                    .show_value(false),
            );
            ui.label("Lower = more compression / smaller file.");
            ui.label("Higher = less compression / better quality.");

            ui.add_space(20.0);
            ui.separator();
            ui.add_space(10.0);

            // Compress button
            let can_compress = self.input_path.is_some() 
                && self.output_path.is_some() 
                && !self.is_compressing;
            
            if ui
                .add_enabled(can_compress, egui::Button::new("Compress image"))
                .clicked()
            {
                if let (Some(input), Some(output)) = (&self.input_path, &self.output_path) {
                    let task = CompressionTask {
                        input_path: input.clone(),
                        output_path: output.clone(),
                        quality: self.quality,
                    };
                    let _ = self.compress_tx.send(task);
                    self.is_compressing = true;
                    self.status_message = "Compressing...".to_string();
                }
            }

            ui.add_space(10.0);

            // Status
            ui.horizontal(|ui| {
                ui.label("Status:");
                let color = if self.status_message.starts_with("Error") {
                    egui::Color32::RED
                } else if self.status_message.starts_with("Success") {
                    egui::Color32::GREEN
                } else {
                    egui::Color32::GRAY
                };
                ui.colored_label(color, &self.status_message);
            });
        });

        // Request repaint to check for results
        if self.is_compressing {
            ctx.request_repaint();
        }
    }
}

// Compression logic running in background thread
fn perform_compression(task: CompressionTask) -> Result<String, String> {
    let img = match image::open(&task.input_path) {
        Ok(img) => img,
        Err(e) => return Err(format!("Error loading image: {e}")),
    };

    let format = match task.output_path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_lowercase())
        .as_deref()
    {
        Some("jpg") | Some("jpeg") => ImageFormat::Jpeg,
        Some("png") => ImageFormat::Png,
        Some("webp") => ImageFormat::WebP,
        _ => {
            return Err("Error: unsupported format. Use .jpg, .png, or .webp".to_string());
        }
    };

    let res = match format {
        ImageFormat::Jpeg => save_jpeg(&img, &task.output_path, task.quality),
        ImageFormat::Png => save_png(&img, &task.output_path, task.quality),
        ImageFormat::WebP => save_webp_lossless(&img, &task.output_path),
        _ => {
            return Err("Error: unsupported format".to_string());
        }
    };

    match res {
        Ok(_) => Ok(format!("Success: saved to {}", task.output_path.display())),
        Err(e) => Err(format!("Error saving image: {e}")),
    }
}

// Helper functions

fn save_jpeg(
    img: &DynamicImage,
    path: &PathBuf,
    quality: u8,
) -> Result<(), image::ImageError> {
    use image::codecs::jpeg::JpegEncoder;
    use std::fs::File;
    use std::io::BufWriter;

    let file = File::create(path)?;
    let writer = BufWriter::new(file);
    let mut encoder = JpegEncoder::new_with_quality(writer, quality);
    encoder.encode_image(img)
}

fn save_png(
    img: &DynamicImage,
    path: &PathBuf,
    quality: u8,
) -> Result<(), image::ImageError> {
    use image::codecs::png::{PngEncoder, CompressionType, FilterType};
    use image::{ColorType, ImageEncoder};
    use std::fs::File;
    use std::io::BufWriter;

    let file = File::create(path)?;
    let writer = BufWriter::new(file);

    // Map quality to compression type
    let compression = if quality < 40 {
        CompressionType::Fast
    } else if quality < 80 {
        CompressionType::Default
    } else {
        CompressionType::Best
    };

    let encoder = PngEncoder::new_with_quality(
        writer,
        compression,
        FilterType::Adaptive,
    );

    // Get raw RGBA8 data from the DynamicImage.
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();

    // Use the ImageEncoder::write_image method implemented by PngEncoder.
    encoder.write_image(
        &rgba,
        width,
        height,
        ColorType::Rgba8.into(),
    )
}

fn save_webp_lossless(
    img: &DynamicImage,
    path: &PathBuf,
) -> Result<(), image::ImageError> {
    use image::codecs::webp::WebPEncoder;
    use image::{ExtendedColorType, ImageEncoder};
    use std::fs::File;
    use std::io::BufWriter;

    let file = File::create(path)?;
    let writer = BufWriter::new(file);

    // WebPEncoder in image 0.25 only supports lossless encoding and requires Rgb8/Rgba8 data.
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();

    let encoder = WebPEncoder::new_lossless(writer);

    // Either `encode` or `write_image` is fine; both take the same arguments.
    encoder.write_image(
        &rgba,
        width,
        height,
        ExtendedColorType::Rgba8,
    )
}
