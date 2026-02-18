use std::path::Path;

use anyhow::{anyhow, Result};
use log::{debug, trace};
use chrono::NaiveDateTime;
use reverse_geocoder::ReverseGeocoder;
use windows::core::HSTRING;
use windows::Graphics::Imaging::BitmapDecoder;
use windows::Media::Ocr::OcrEngine;
use windows::Storage::{FileAccessMode, StorageFile};

pub fn is_image_extension(ext: &str) -> bool {
    matches!(ext, "png" | "jpg" | "jpeg" | "bmp" | "tiff" | "tif" | "gif" | "webp")
}

pub async fn extract_text_from_image(path: &Path) -> Result<String> {
    debug!("OCR processing: {}", path.display());
    let ocr_text = run_ocr(path).await.unwrap_or_default();
    let exif_text = extract_exif_metadata(path).unwrap_or_default();

    let mut parts = Vec::new();
    if !ocr_text.trim().is_empty() {
        parts.push(ocr_text.trim().to_string());
    }
    if !exif_text.is_empty() {
        parts.push(exif_text);
    }

    if parts.is_empty() {
        return Err(anyhow!("No text or metadata found"));
    }

    trace!("OCR result for {}: {} bytes", path.display(), parts.iter().map(|p| p.len()).sum::<usize>());
    Ok(parts.join("\n\n"))
}

async fn run_ocr(path: &Path) -> Result<String> {
    let abs_path = std::fs::canonicalize(path)
        .map_err(|e| anyhow!("Failed to canonicalize path: {}", e))?;
    let path_str = abs_path.to_string_lossy().to_string();
    let path_str = path_str.strip_prefix(r"\\?\").unwrap_or(&path_str).to_string();

    let hpath = HSTRING::from(&path_str);
    let file = StorageFile::GetFileFromPathAsync(&hpath)?
        .get()
        .map_err(|e| anyhow!("Failed to open file: {}", e))?;

    let stream = file.OpenAsync(FileAccessMode::Read)?
        .get()
        .map_err(|e| anyhow!("Failed to open stream: {}", e))?;

    let decoder = BitmapDecoder::CreateAsync(&stream)?
        .get()
        .map_err(|e| anyhow!("Failed to create decoder: {}", e))?;

    let bitmap = decoder.GetSoftwareBitmapAsync()?
        .get()
        .map_err(|e| anyhow!("Failed to get bitmap: {}", e))?;

    let engine = OcrEngine::TryCreateFromUserProfileLanguages()
        .map_err(|e| anyhow!("Failed to create OCR engine: {}", e))?;

    let result = engine.RecognizeAsync(&bitmap)?
        .get()
        .map_err(|e| anyhow!("OCR recognition failed: {}", e))?;

    let text = result.Text()
        .map_err(|e| anyhow!("Failed to get OCR text: {}", e))?;

    Ok(text.to_string())
}

fn extract_exif_metadata(path: &Path) -> Result<String> {
    let file = std::fs::File::open(path)?;
    let mut buf = std::io::BufReader::new(file);
    let exif = exif::Reader::new().read_from_container(&mut buf)
        .map_err(|e| anyhow!("EXIF read failed: {}", e))?;

    let mut parts: Vec<String> = Vec::new();

    if let Some(f) = exif.get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY)
        .or_else(|| exif.get_field(exif::Tag::DateTime, exif::In::PRIMARY))
    {
        let raw = f.display_value().to_string().replace('"', "");
        parts.push(format_date_human(&raw));
    }

    if let Some(f) = exif.get_field(exif::Tag::Make, exif::In::PRIMARY) {
        let make = f.display_value().to_string().replace('"', "");
        if let Some(m) = exif.get_field(exif::Tag::Model, exif::In::PRIMARY) {
            let model = m.display_value().to_string().replace('"', "");
            parts.push(format!("Camera: {} {}", make.trim(), model.trim()));
        } else {
            parts.push(format!("Camera: {}", make.trim()));
        }
    }
    if let Some(f) = exif.get_field(exif::Tag::LensModel, exif::In::PRIMARY) {
        parts.push(format!("Lens: {}", f.display_value().to_string().replace('"', "")));
    }

    if let Some(f) = exif.get_field(exif::Tag::FNumber, exif::In::PRIMARY) {
        parts.push(format!("f/{}", f.display_value()));
    }
    if let Some(f) = exif.get_field(exif::Tag::ExposureTime, exif::In::PRIMARY) {
        parts.push(format!("{}s", f.display_value()));
    }
    if let Some(f) = exif.get_field(exif::Tag::PhotographicSensitivity, exif::In::PRIMARY) {
        parts.push(format!("ISO {}", f.display_value()));
    }
    if let Some(f) = exif.get_field(exif::Tag::FocalLength, exif::In::PRIMARY) {
        parts.push(format!("{}mm", f.display_value()));
    }

    let lat = parse_gps_coord(&exif, exif::Tag::GPSLatitude, exif::Tag::GPSLatitudeRef);
    let lon = parse_gps_coord(&exif, exif::Tag::GPSLongitude, exif::Tag::GPSLongitudeRef);
    if let (Some(lat), Some(lon)) = (lat, lon) {
        let location = reverse_geocode(lat, lon);
        parts.push(format!("Location: {}", location));
    }

    if let Some(f) = exif.get_field(exif::Tag::Artist, exif::In::PRIMARY) {
        parts.push(format!("Artist: {}", f.display_value().to_string().replace('"', "")));
    }
    if let Some(f) = exif.get_field(exif::Tag::Copyright, exif::In::PRIMARY) {
        parts.push(format!("Copyright: {}", f.display_value().to_string().replace('"', "")));
    }
    if let Some(f) = exif.get_field(exif::Tag::ImageDescription, exif::In::PRIMARY) {
        parts.push(f.display_value().to_string().replace('"', ""));
    }

    if parts.is_empty() {
        return Err(anyhow!("No EXIF metadata found"));
    }

    Ok(parts.join(" | "))
}

fn format_date_human(raw: &str) -> String {
    let cleaned = raw.trim();

    if let Ok(dt) = NaiveDateTime::parse_from_str(cleaned, "%Y-%m-%d %H:%M:%S") {
        return build_date_string(dt);
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(cleaned, "%Y:%m:%d %H:%M:%S") {
        return build_date_string(dt);
    }

    format!("Date: {}", cleaned)
}

fn build_date_string(dt: NaiveDateTime) -> String {
    let months_tr = [
        "", "Ocak", "Şubat", "Mart", "Nisan", "Mayıs", "Haziran",
        "Temmuz", "Ağustos", "Eylül", "Ekim", "Kasım", "Aralık",
    ];
    let months_en = [
        "", "January", "February", "March", "April", "May", "June",
        "July", "August", "September", "October", "November", "December",
    ];
    let days_tr = ["Pazartesi", "Salı", "Çarşamba", "Perşembe", "Cuma", "Cumartesi", "Pazar"];
    let days_en = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];

    let month_idx = dt.format("%m").to_string().parse::<usize>().unwrap_or(0);
    let day_of_week = dt.format("%u").to_string().parse::<usize>().unwrap_or(1) - 1;
    let hour = dt.format("%H").to_string().parse::<u32>().unwrap_or(12);

    let time_of_day = match hour {
        5..=11 => "morning, sabah",
        12..=16 => "afternoon, öğleden sonra",
        17..=20 => "evening, akşam",
        _ => "night, gece",
    };

    let season = match month_idx {
        3..=5 => "spring, ilkbahar",
        6..=8 => "summer, yaz",
        9..=11 => "autumn, sonbahar",
        _ => "winter, kış",
    };

    let month_tr = months_tr.get(month_idx).unwrap_or(&"");
    let month_en = months_en.get(month_idx).unwrap_or(&"");
    let day_tr = days_tr.get(day_of_week).unwrap_or(&"");
    let day_en = days_en.get(day_of_week).unwrap_or(&"");

    let day = dt.format("%d").to_string();
    let year = dt.format("%Y").to_string();
    let time = dt.format("%H:%M").to_string();

    format!(
        "{} {} {} {}, {} {}, {} {}, {}",
        day, month_tr, month_en, year,
        day_tr, day_en,
        time, time_of_day,
        season,
    )
}

fn reverse_geocode(lat: f64, lon: f64) -> String {
    use std::sync::LazyLock;
    static GEOCODER: LazyLock<ReverseGeocoder> = LazyLock::new(ReverseGeocoder::new);

    let result = GEOCODER.search((lat, lon));

    let city = &result.record.name;
    let admin = &result.record.admin1;
    let country = &result.record.cc;
    if admin.is_empty() {
        format!("{}, {}", city, country)
    } else {
        format!("{}, {}, {}", city, admin, country)
    }
}

fn parse_gps_coord(exif: &exif::Exif, coord_tag: exif::Tag, ref_tag: exif::Tag) -> Option<f64> {
    let field = exif.get_field(coord_tag, exif::In::PRIMARY)?;
    let values: Vec<f64> = match &field.value {
        exif::Value::Rational(rats) => {
            if rats.iter().any(|r| r.denom == 0) {
                return None;
            }
            rats.iter().map(|r| r.num as f64 / r.denom as f64).collect()
        }
        _ => return None,
    };
    if values.len() < 3 {
        return None;
    }
    let mut coord = values[0] + values[1] / 60.0 + values[2] / 3600.0;

    if let Some(ref_field) = exif.get_field(ref_tag, exif::In::PRIMARY) {
        let ref_str = ref_field.display_value().to_string();
        if ref_str.contains('S') || ref_str.contains('W') {
            coord = -coord;
        }
    }
    Some(coord)
}
