//! Minimal PowerPoint (.pptx) writer — generates a real OOXML deck from the
//! same structured `Deck` model the HTML renderer uses (E).
//!
//! Why hand-rolled: the JSON model stays the single source of truth, so this is
//! a pure *exporter* — it never becomes an editing path, and the "small tweaks
//! stay predictable" property is preserved. A pptx is just a ZIP of XML parts;
//! we emit one fixed master/layout/theme plus one `slideN.xml` per slide.
//!
//! Scope: text (title/subtitle/bullets/body/quote/columns), pictures, speaker
//! notes are omitted (PowerPoint renders fine without a notes part). Everything
//! is absolutely positioned in EMU, mirroring the 1280×720 CSS canvas.

use std::io::{Cursor, Write};

use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use crate::slides::{Brand, Deck, Slide};

/// 1 px on our 1280×720 canvas → EMU (12192000 EMU = 13.333in = 1280px).
const PX: i64 = 9525;
const SLIDE_W: i64 = 1280 * PX;
const SLIDE_H: i64 = 720 * PX;

fn xml_esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// `**bold**` → separate runs; everything else is a plain run.
fn runs(text: &str, size_pt: i64, color: &str, bold_all: bool) -> String {
    let mut out = String::new();
    let mut rest = text;
    let mut emit = |t: &str, bold: bool| {
        if t.is_empty() {
            return;
        }
        out.push_str(&format!(
            r#"<a:r><a:rPr lang="zh-CN" sz="{}" b="{}" dirty="0"><a:solidFill><a:srgbClr val="{}"/></a:solidFill></a:rPr><a:t>{}</a:t></a:r>"#,
            size_pt * 100,
            if bold || bold_all { 1 } else { 0 },
            color,
            xml_esc(t)
        ));
    };
    while let Some(open) = rest.find("**") {
        emit(&rest[..open], false);
        let after = &rest[open + 2..];
        match after.find("**") {
            Some(close) => {
                emit(&after[..close], true);
                rest = &after[close + 2..];
            }
            None => {
                rest = after;
                break;
            }
        }
    }
    emit(rest, false);
    if out.is_empty() {
        out.push_str(r#"<a:endParaRPr lang="zh-CN"/>"#);
    }
    out
}

fn para(text: &str, size_pt: i64, color: &str, bold: bool, bullet: bool) -> String {
    let props = if bullet {
        r#"<a:pPr marL="285750" indent="-285750"><a:buChar char="•"/></a:pPr>"#
    } else {
        r#"<a:pPr><a:buNone/></a:pPr>"#
    };
    format!("<a:p>{props}{}</a:p>", runs(text, size_pt, color, bold))
}

/// One absolutely-positioned text box.
fn text_box(id: u32, name: &str, x: i64, y: i64, w: i64, h: i64, paras: &str) -> String {
    format!(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="{name}"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{w}" cy="{h}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/></p:spPr><p:txBody><a:bodyPr wrap="square" anchor="t"><a:normAutofit/></a:bodyPr><a:lstStyle/>{paras}</p:txBody></p:sp>"#
    )
}

fn pic_shape(id: u32, rel_id: &str, x: i64, y: i64, w: i64, h: i64) -> String {
    format!(
        r#"<p:pic><p:nvPicPr><p:cNvPr id="{id}" name="Picture {id}"/><p:cNvPicPr/><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed="{rel_id}"/><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr><a:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{w}" cy="{h}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr></p:pic>"#
    )
}

/// Theme → (background, title color, body color, accent) as hex without '#'.
fn palette(theme: &str, brand: Option<&Brand>) -> (String, String, String, String) {
    let (bg, title, body, accent) = match theme {
        "minimal" => ("FFFFFF", "1A1A2E", "5A5A72", "111111"),
        "corporate" => ("F4F7FB", "0F2540", "3D5A80", "2F6FED"),
        "sunset" => ("2B1055", "FFFFFF", "FFE0C7", "FF8F6B"),
        _ => ("0B1020", "EAF0FF", "AAB8DD", "4DD0E1"), // midnight
    };
    let clean = |v: &str, fallback: &str| -> String {
        let t = v.trim().trim_start_matches('#');
        // Only accept plain 6-digit hex (CSS gradients can't map to pptx fills).
        if t.len() == 6 && t.chars().all(|c| c.is_ascii_hexdigit()) {
            t.to_uppercase()
        } else {
            fallback.to_string()
        }
    };
    match brand {
        Some(b) => (
            clean(&b.background, bg),
            clean(&b.primary, title),
            clean(&b.text, body),
            clean(&b.accent, accent),
        ),
        None => (bg.into(), title.into(), body.into(), accent.into()),
    }
}

struct SlideMedia {
    file: String,
    bytes: Vec<u8>,
    ext: String,
}

/// Build one `slideN.xml` (+ any embedded picture) from a `Slide`.
fn slide_xml(slide: &Slide, deck: &Deck, media: &mut Option<SlideMedia>) -> String {
    let (bg, title_c, body_c, accent_c) = palette(&deck.theme, deck.brand.as_ref());
    let mut shapes = String::new();
    let mut id = 2u32;

    // Picture (if any) — read the local file / skip remote URLs (pptx must embed).
    let has_pic = {
        let r = slide.image.trim();
        if !r.is_empty() && !r.starts_with("http") && !r.starts_with("data:") {
            if let Ok(bytes) = std::fs::read(r) {
                let ext = std::path::Path::new(r)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("png")
                    .to_ascii_lowercase();
                *media = Some(SlideMedia {
                    file: String::new(), // filled by caller (needs slide index)
                    bytes,
                    ext,
                });
                true
            } else {
                false
            }
        } else {
            false
        }
    };

    let layout = slide.layout.as_str();
    match layout {
        "cover" | "section" => {
            let (ty, size) = if layout == "cover" { (240, 54) } else { (280, 44) };
            shapes.push_str(&text_box(
                id, "Title", 96 * PX, ty * PX, 1088 * PX, 160 * PX,
                &para(&slide.title, size, &title_c, true, false),
            ));
            id += 1;
            if !slide.subtitle.is_empty() {
                shapes.push_str(&text_box(
                    id, "Subtitle", 96 * PX, (ty + 170) * PX, 1088 * PX, 80 * PX,
                    &para(&slide.subtitle, 22, &body_c, false, false),
                ));
                id += 1;
            }
        }
        "quote" => {
            shapes.push_str(&text_box(
                id, "Quote", 96 * PX, 220 * PX, 1088 * PX, 220 * PX,
                &para(&slide.body, 32, &title_c, true, false),
            ));
            id += 1;
            if !slide.subtitle.is_empty() {
                shapes.push_str(&text_box(
                    id, "Cite", 96 * PX, 460 * PX, 1088 * PX, 60 * PX,
                    &para(&format!("— {}", slide.subtitle), 20, &body_c, false, false),
                ));
                id += 1;
            }
        }
        "two-column" => {
            shapes.push_str(&text_box(
                id, "Title", 96 * PX, 80 * PX, 1088 * PX, 80 * PX,
                &para(&slide.title, 36, &title_c, true, false),
            ));
            id += 1;
            for (ci, col) in slide.columns.iter().take(2).enumerate() {
                let x = (96 + ci as i64 * 552) * PX;
                let mut p = para(&col.title, 22, &accent_c, true, false);
                if !col.body.is_empty() {
                    p.push_str(&para(&col.body, 16, &body_c, false, false));
                }
                for b in &col.bullets {
                    p.push_str(&para(b, 16, &body_c, false, true));
                }
                shapes.push_str(&text_box(id, "Col", x, 190 * PX, 496 * PX, 440 * PX, &p));
                id += 1;
            }
        }
        _ => {
            // content / bullets / image / image-left share a title+body layout.
            let text_w = if has_pic && layout == "image-left" { 560 } else { 1088 };
            let text_x = if has_pic && layout == "image-left" { 620 } else { 96 };
            if !slide.title.is_empty() {
                shapes.push_str(&text_box(
                    id, "Title", text_x * PX, 80 * PX, text_w * PX, 90 * PX,
                    &para(&slide.title, 36, &title_c, true, false),
                ));
                id += 1;
            }
            let mut p = String::new();
            if !slide.subtitle.is_empty() {
                p.push_str(&para(&slide.subtitle, 20, &body_c, false, false));
            }
            for b in &slide.bullets {
                p.push_str(&para(b, 18, &body_c, false, true));
            }
            for line in slide.body.split('\n').filter(|l| !l.trim().is_empty()) {
                p.push_str(&para(line, 18, &body_c, false, false));
            }
            if !p.is_empty() {
                shapes.push_str(&text_box(
                    id, "Body", text_x * PX, 190 * PX, text_w * PX, 440 * PX, &p,
                ));
                id += 1;
            }
            if has_pic {
                let (px, py, pw, ph) = match layout {
                    "image-left" => (0, 0, 560, 720),
                    _ => (340, 250, 600, 380), // centered under the title
                };
                shapes.push_str(&pic_shape(id, "rId2", px * PX, py * PX, pw * PX, ph * PX));
                id += 1;
            }
        }
    }

    // Brand footer as a plain text box (logo is skipped: one picture rel per slide).
    if let Some(b) = &deck.brand {
        if !b.footer.trim().is_empty() {
            shapes.push_str(&text_box(
                id, "Footer", 96 * PX, 660 * PX, 700 * PX, 40 * PX,
                &para(&b.footer, 11, &body_c, false, false),
            ));
        }
    }

    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:bg><p:bgPr><a:solidFill><a:srgbClr val="{bg}"/></a:solidFill><a:effectLst/></p:bgPr></p:bg><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>{shapes}</p:spTree></p:cSld><p:clrMapOvr><a:overrideClrMapping bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/></p:clrMapOvr></p:sld>"#
    )
}

const THEME_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="OMNIX"><a:themeElements><a:clrScheme name="OMNIX"><a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1><a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1><a:dk2><a:srgbClr val="44546A"/></a:dk2><a:lt2><a:srgbClr val="E7E6E6"/></a:lt2><a:accent1><a:srgbClr val="4DD0E1"/></a:accent1><a:accent2><a:srgbClr val="2F6FED"/></a:accent2><a:accent3><a:srgbClr val="A5A5A5"/></a:accent3><a:accent4><a:srgbClr val="FFC000"/></a:accent4><a:accent5><a:srgbClr val="5B9BD5"/></a:accent5><a:accent6><a:srgbClr val="70AD47"/></a:accent6><a:hlink><a:srgbClr val="0563C1"/></a:hlink><a:folHlink><a:srgbClr val="954F72"/></a:folHlink></a:clrScheme><a:fontScheme name="OMNIX"><a:majorFont><a:latin typeface="Inter"/><a:ea typeface="Microsoft YaHei"/><a:cs typeface=""/></a:majorFont><a:minorFont><a:latin typeface="Inter"/><a:ea typeface="Microsoft YaHei"/><a:cs typeface=""/></a:minorFont></a:fontScheme><a:fmtScheme name="OMNIX"><a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:fillStyleLst><a:lnStyleLst><a:ln w="6350"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="12700"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="19050"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln></a:lnStyleLst><a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst><a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:bgFillStyleLst></a:fmtScheme></a:themeElements></a:theme>"#;

const SLIDE_MASTER_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sldMaster xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:bg><p:bgPr><a:solidFill><a:schemeClr val="bg1"/></a:solidFill><a:effectLst/></p:bgPr></p:bg><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr></p:spTree></p:cSld><p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/><p:sldLayoutIdLst><p:sldLayoutId id="2147483649" r:id="rId1"/></p:sldLayoutIdLst></p:sldMaster>"#;

const SLIDE_LAYOUT_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sldLayout xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" type="blank" preserve="1"><p:cSld name="Blank"><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr></p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>"#;

/// Serialize a `Deck` to .pptx bytes.
pub fn build_pptx(deck: &Deck) -> Result<Vec<u8>, String> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let opt = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let add = |zip: &mut ZipWriter<Cursor<Vec<u8>>>, name: &str, data: &[u8]| -> Result<(), String> {
        zip.start_file(name, opt).map_err(|e| e.to_string())?;
        zip.write_all(data).map_err(|e| e.to_string())?;
        Ok(())
    };

    // Render slides first: we need each slide's media before writing manifests.
    let mut slides_xml = Vec::new();
    let mut medias: Vec<Option<SlideMedia>> = Vec::new();
    for (i, s) in deck.slides.iter().enumerate() {
        let mut m: Option<SlideMedia> = None;
        let xml = slide_xml(s, deck, &mut m);
        if let Some(m) = m.as_mut() {
            m.file = format!("image{}.{}", i + 1, m.ext);
        }
        slides_xml.push(xml);
        medias.push(m);
    }
    let n = slides_xml.len();

    // [Content_Types].xml
    let mut ct = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Default Extension="jpg" ContentType="image/jpeg"/><Default Extension="jpeg" ContentType="image/jpeg"/><Default Extension="gif" ContentType="image/gif"/><Default Extension="webp" ContentType="image/webp"/><Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/><Override PartName="/ppt/slideMasters/slideMaster1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"/><Override PartName="/ppt/slideLayouts/slideLayout1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/><Override PartName="/ppt/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/>"#,
    );
    for i in 1..=n {
        ct.push_str(&format!(
            r#"<Override PartName="/ppt/slides/slide{i}.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/>"#
        ));
    }
    ct.push_str("</Types>");
    add(&mut zip, "[Content_Types].xml", ct.as_bytes())?;

    add(
        &mut zip,
        "_rels/.rels",
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="ppt/presentation.xml"/></Relationships>"#,
    )?;

    // presentation.xml — slide id list + 16:9 size.
    let mut ids = String::new();
    for i in 1..=n {
        ids.push_str(&format!(
            r#"<p:sldId id="{}" r:id="rId{}"/>"#,
            255 + i,
            i + 1
        ));
    }
    add(
        &mut zip,
        "ppt/presentation.xml",
        format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:presentation xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" saveSubsetFonts="1"><p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst><p:sldIdLst>{ids}</p:sldIdLst><p:sldSz cx="{SLIDE_W}" cy="{SLIDE_H}"/><p:notesSz cx="{SLIDE_H}" cy="{SLIDE_W}"/></p:presentation>"#
        )
        .as_bytes(),
    )?;

    // presentation rels: master + every slide + theme.
    let mut prels = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="slideMasters/slideMaster1.xml"/>"#,
    );
    for i in 1..=n {
        prels.push_str(&format!(
            r#"<Relationship Id="rId{}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide{i}.xml"/>"#,
            i + 1
        ));
    }
    prels.push_str(&format!(
        r#"<Relationship Id="rId{}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="theme/theme1.xml"/></Relationships>"#,
        n + 2
    ));
    add(&mut zip, "ppt/_rels/presentation.xml.rels", prels.as_bytes())?;

    add(&mut zip, "ppt/theme/theme1.xml", THEME_XML.as_bytes())?;
    add(&mut zip, "ppt/slideMasters/slideMaster1.xml", SLIDE_MASTER_XML.as_bytes())?;
    add(
        &mut zip,
        "ppt/slideMasters/_rels/slideMaster1.xml.rels",
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="../theme/theme1.xml"/></Relationships>"#,
    )?;
    add(&mut zip, "ppt/slideLayouts/slideLayout1.xml", SLIDE_LAYOUT_XML.as_bytes())?;
    add(
        &mut zip,
        "ppt/slideLayouts/_rels/slideLayout1.xml.rels",
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="../slideMasters/slideMaster1.xml"/></Relationships>"#,
    )?;

    for (i, xml) in slides_xml.iter().enumerate() {
        add(&mut zip, &format!("ppt/slides/slide{}.xml", i + 1), xml.as_bytes())?;
        let mut rels = String::from(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>"#,
        );
        if let Some(m) = &medias[i] {
            rels.push_str(&format!(
                r#"<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/{}"/>"#,
                m.file
            ));
        }
        rels.push_str("</Relationships>");
        add(&mut zip, &format!("ppt/slides/_rels/slide{}.xml.rels", i + 1), rels.as_bytes())?;
    }
    for m in medias.iter().flatten() {
        add(&mut zip, &format!("ppt/media/{}", m.file), &m.bytes)?;
    }

    let cursor = zip.finish().map_err(|e| e.to_string())?;
    Ok(cursor.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slides::Slide;

    fn deck() -> Deck {
        Deck {
            id: "d".into(),
            title: "T".into(),
            theme: "midnight".into(),
            brand: None,
            slides: vec![
                Slide { layout: "cover".into(), title: "封面 & <标题>".into(), subtitle: "副标题".into(), ..Default::default() },
                Slide { layout: "bullets".into(), title: "要点".into(), bullets: vec!["一 **重点**".into(), "二".into()], ..Default::default() },
            ],
        }
    }

    #[test]
    fn pptx_is_a_zip_with_required_parts() {
        let bytes = build_pptx(&deck()).unwrap();
        assert_eq!(&bytes[..2], b"PK", "pptx must be a zip");
        let mut z = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let names: Vec<String> = (0..z.len()).map(|i| z.by_index(i).unwrap().name().to_string()).collect();
        for required in [
            "[Content_Types].xml",
            "_rels/.rels",
            "ppt/presentation.xml",
            "ppt/_rels/presentation.xml.rels",
            "ppt/slideMasters/slideMaster1.xml",
            "ppt/slideLayouts/slideLayout1.xml",
            "ppt/theme/theme1.xml",
            "ppt/slides/slide1.xml",
            "ppt/slides/slide2.xml",
            "ppt/slides/_rels/slide1.xml.rels",
        ] {
            assert!(names.contains(&required.to_string()), "missing {required}");
        }
    }

    #[test]
    fn slide_xml_escapes_and_bolds() {
        let bytes = build_pptx(&deck()).unwrap();
        let mut z = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut s = String::new();
        use std::io::Read;
        z.by_name("ppt/slides/slide1.xml").unwrap().read_to_string(&mut s).unwrap();
        assert!(s.contains("封面 &amp; &lt;标题&gt;"), "xml-escaped");

        let mut s2 = String::new();
        let bytes2 = build_pptx(&deck()).unwrap();
        let mut z2 = zip::ZipArchive::new(Cursor::new(bytes2)).unwrap();
        z2.by_name("ppt/slides/slide2.xml").unwrap().read_to_string(&mut s2).unwrap();
        assert!(s2.contains(r#"b="1""#), "**bold** becomes a bold run");
        assert!(s2.contains("<a:buChar char=\"•\"/>"), "bullets get bullet chars");
    }

    #[test]
    fn brand_hex_overrides_palette_and_ignores_gradients() {
        let mut d = deck();
        d.brand = Some(Brand { primary: "#ff0000".into(), background: "linear-gradient(x)".into(), ..Default::default() });
        let (bg, title, _, _) = palette(&d.theme, d.brand.as_ref());
        assert_eq!(title, "FF0000", "brand hex wins");
        assert_eq!(bg, "0B1020", "non-hex gradient falls back to theme");
    }
}
