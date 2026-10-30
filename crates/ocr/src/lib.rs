use aether_core::{AetherError, AetherResult, DecodedFrame, OcrLanguageMode};
use image::{ImageFormat, RgbaImage};
use lopdf::{dictionary, Document, Object, Stream};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextBoundingBox {
    pub norm_x: f32,
    pub norm_y: f32,
    pub norm_w: f32,
    pub norm_h: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrTextBlock {
    pub id: usize,
    pub text_logical: String,
    pub text_visual_display: String,
    pub is_rtl: bool,
    pub confidence: f32,
    pub bbox: TextBoundingBox,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrDocumentResult {
    pub execution_provider: String,
    pub language_mode: OcrLanguageMode,
    pub blocks: Vec<OcrTextBlock>,
    pub full_text: String,
    pub inference_ms: u64,
}

pub struct OcrEngine {
    models_dir: PathBuf,
}

impl OcrEngine {
    pub fn new(models_dir: PathBuf, _use_cuda: bool) -> Self {
        Self { models_dir }
    }

    pub fn execution_provider_label(&self) -> &'static str {
        "Tesseract OCR Engine (LSTM Neural Net • ara+eng)"
    }

    pub fn models_status(&self) -> (bool, PathBuf) {
        let ok = Command::new("tesseract").arg("--version").output().is_ok();
        (ok, self.models_dir.clone())
    }

    pub fn contains_arabic(text: &str) -> bool {
        text.chars().any(|c| {
            let u = c as u32;
            (0x0600..=0x06FF).contains(&u)
                || (0x0750..=0x077F).contains(&u)
                || (0x08A0..=0x08FF).contains(&u)
                || (0xFB50..=0xFDFF).contains(&u)
                || (0xFE70..=0xFEFF).contains(&u)
        })
    }

    fn resolve_tesseract_lang_arg(lang: OcrLanguageMode) -> AetherResult<String> {
        let output = Command::new("tesseract")
            .arg("--list-langs")
            .output()
            .map_err(|e| {
                AetherError::OcrError(format!(
                    "Tesseract not installed ({}). Run: sudo pacman -S tesseract tesseract-data-ara tesseract-data-eng",
                    e
                ))
            })?;

        let combined = String::from_utf8_lossy(&output.stdout).to_string()
            + &String::from_utf8_lossy(&output.stderr);

        let has_ara = combined.lines().any(|l| l.trim() == "ara");
        let has_eng = combined.lines().any(|l| l.trim() == "eng");

        if !has_ara && !has_eng {
            return Err(AetherError::OcrError(
                "No Tesseract language data found! Run: sudo pacman -S tesseract-data-ara tesseract-data-eng"
                    .into(),
            ));
        }

        match lang {
            OcrLanguageMode::ArabicOnly => {
                if has_ara {
                    Ok("ara".to_string())
                } else {
                    Err(AetherError::OcrError(
                        "Arabic OCR pack missing! Install via: sudo pacman -S tesseract-data-ara"
                            .into(),
                    ))
                }
            }
            OcrLanguageMode::EnglishOnly => {
                if has_eng {
                    Ok("eng".to_string())
                } else {
                    Ok("ara".to_string())
                }
            }
            OcrLanguageMode::ArabicAndEnglish | OcrLanguageMode::Auto => {
                if has_ara && has_eng {
                    Ok("ara+eng".to_string())
                } else if has_ara {
                    Ok("ara".to_string())
                } else {
                    Ok("eng".to_string())
                }
            }
        }
    }

    /// Runs Tesseract OCR using a safe temporary PNG file (avoiding stdin pipe truncation on large images)
    /// and automatically retries with `--psm 11` (Sparse Text mode for Manga/Comics bubbles) if page mode yields 0 blocks.
    pub fn recognize_frame(
        &self,
        frame: &DecodedFrame,
        lang: OcrLanguageMode,
        cancel_token: &CancellationToken,
    ) -> AetherResult<OcrDocumentResult> {
        let started = std::time::Instant::now();

        if cancel_token.is_cancelled() {
            return Err(AetherError::Cancelled);
        }

        let lang_arg = Self::resolve_tesseract_lang_arg(lang)?;

        let rgba = RgbaImage::from_raw(
            frame.width,
            frame.height,
            frame.rgba_pixels.as_ref().clone(),
        )
        .ok_or_else(|| AetherError::OcrError("Invalid image buffer for OCR".into()))?;

        let temp_path = std::env::temp_dir().join(format!(
            "aetherview_ocr_{}.png",
            std::process::id()
        ));

        rgba.save_with_format(&temp_path, ImageFormat::Png)
            .map_err(|e| AetherError::OcrError(format!("Failed to stage temp PNG for OCR: {}", e)))?;

        // First try PSM 11 (Sparse Text — best for Manga bubbles, screenshots, and mixed layouts)
        let mut blocks = self.run_tesseract_tsv(&temp_path, &lang_arg, "11", frame.width, frame.height)?;

        // Fallback to PSM 3 (Fully automatic page segmentation) if PSM 11 returned nothing
        if blocks.is_empty() && !cancel_token.is_cancelled() {
            blocks = self.run_tesseract_tsv(&temp_path, &lang_arg, "3", frame.width, frame.height)?;
        }

        let _ = fs::remove_file(&temp_path);

        if cancel_token.is_cancelled() {
            return Err(AetherError::Cancelled);
        }

        let full_text = blocks
            .iter()
            .map(|b| b.text_logical.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        Ok(OcrDocumentResult {
            execution_provider: format!("Tesseract LSTM ({})", lang_arg),
            language_mode: lang,
            blocks,
            full_text,
            inference_ms: started.elapsed().as_millis() as u64,
        })
    }

    fn run_tesseract_tsv(
        &self,
        img_path: &Path,
        lang_arg: &str,
        psm: &str,
        width: u32,
        height: u32,
    ) -> AetherResult<Vec<OcrTextBlock>> {
        let output = Command::new("tesseract")
            .arg(img_path)
            .arg("stdout")
            .args(["-l", lang_arg, "--oem", "1", "--psm", psm, "tsv"])
            .output()
            .map_err(|e| AetherError::OcrError(format!("Failed to execute tesseract: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AetherError::OcrError(format!(
                "Tesseract failed: {}",
                stderr.trim()
            )));
        }

        let tsv_text = String::from_utf8_lossy(&output.stdout);
        let img_w = width.max(1) as f32;
        let img_h = height.max(1) as f32;

        struct LineAccum {
            words: Vec<String>,
            conf_sum: f32,
            count: usize,
            min_x: i32,
            min_y: i32,
            max_r: i32,
            max_b: i32,
        }

        let mut line_map: BTreeMap<(i32, i32, i32), LineAccum> = BTreeMap::new();

        for line in tsv_text.lines().skip(1) {
            let cols: Vec<&str> = line.split('\t').collect();
            if cols.len() < 12 {
                continue;
            }
            let level: i32 = cols[0].parse().unwrap_or(0);
            if level != 5 {
                continue;
            }
            let word = cols[11].trim();
            if word.is_empty() {
                continue;
            }
            let conf: f32 = cols[10].parse().unwrap_or(-1.0);
            if conf < 12.0 {
                continue;
            }

            let block_num: i32 = cols[2].parse().unwrap_or(0);
            let par_num: i32 = cols[3].parse().unwrap_or(0);
            let line_num: i32 = cols[4].parse().unwrap_or(0);

            let left: i32 = cols[6].parse().unwrap_or(0);
            let top: i32 = cols[7].parse().unwrap_or(0);
            let w: i32 = cols[8].parse().unwrap_or(1);
            let h: i32 = cols[9].parse().unwrap_or(1);

            let entry = line_map
                .entry((block_num, par_num, line_num))
                .or_insert_with(|| LineAccum {
                    words: Vec::new(),
                    conf_sum: 0.0,
                    count: 0,
                    min_x: left,
                    min_y: top,
                    max_r: left + w,
                    max_b: top + h,
                });

            entry.words.push(word.to_string());
            entry.conf_sum += conf;
            entry.count += 1;
            entry.min_x = entry.min_x.min(left);
            entry.min_y = entry.min_y.min(top);
            entry.max_r = entry.max_r.max(left + w);
            entry.max_b = entry.max_b.max(top + h);
        }

        let mut blocks = Vec::new();
        for (idx, (_, accum)) in line_map.into_iter().enumerate() {
            let logical = accum.words.join(" ");
            let is_rtl = Self::contains_arabic(&logical);
            let avg_conf = (accum.conf_sum / (accum.count.max(1) as f32) / 100.0).clamp(0.0, 1.0);

            blocks.push(OcrTextBlock {
                id: idx + 1,
                text_visual_display: logical.clone(),
                text_logical: logical,
                is_rtl,
                confidence: avg_conf,
                bbox: TextBoundingBox {
                    norm_x: (accum.min_x as f32 / img_w).clamp(0.0, 1.0),
                    norm_y: (accum.min_y as f32 / img_h).clamp(0.0, 1.0),
                    norm_w: ((accum.max_r - accum.min_x).max(1) as f32 / img_w).clamp(0.01, 1.0),
                    norm_h: ((accum.max_b - accum.min_y).max(1) as f32 / img_h).clamp(0.01, 1.0),
                },
            });
        }

        Ok(blocks)
    }

    pub fn export_searchable_pdf(
        ocr_result: &OcrDocumentResult,
        target_path: &Path,
    ) -> AetherResult<()> {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();

        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        });

        let resources_id = doc.add_object(dictionary! {
            "Font" => dictionary! {
                "F1" => font_id,
            },
        });

        let mut content_ops = String::from("BT\n/F1 12 Tf\n");
        for block in &ocr_result.blocks {
            let pdf_x = (block.bbox.norm_x * 595.0).clamp(36.0, 540.0);
            let pdf_y = ((1.0 - block.bbox.norm_y) * 842.0).clamp(48.0, 800.0);
            let sanitized = block
                .text_logical
                .replace('\\', "\\\\")
                .replace('(', "\\(")
                .replace(')', "\\)");
            content_ops.push_str(&format!(
                "1 0 0 1 {:.1} {:.1} Tm ({}) Tj\n",
                pdf_x, pdf_y, sanitized
            ));
        }
        content_ops.push_str("ET\n");

        let content_id = doc.add_object(Stream::new(dictionary! {}, content_ops.into_bytes()));

        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "Resources" => resources_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.0.into()],
        });

        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        };
        doc.objects.insert(pages_id, Object::Dictionary(pages));

        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);

        doc.save(target_path).map_err(|e| {
            AetherError::OcrError(format!("Failed to export searchable PDF: {}", e))
        })?;
        Ok(())
    }
}
