//! How the board looks to its editor and its previews (design note 4.2): the extras `elementSvg`
//! (fragments by appearance revision), `textMetrics` (the engine's line layout, for the editor's
//! text box and for checking it against the browser), `blob` (a picture's bytes for the page) and
//! `addFont`, and the raster behind `render`/`renderArea`.

use std::sync::Arc;

use kition_board_kdm::Kind;
use kition_board_model::fields::{SHAPE_GROUPS, STYLE_FIELDS, uses};
use kition_board_scene::style::{PALETTE, Scheme, field_default};
use kition_board_scene::{Area, Raster, area};
use kition_board_text::{Family, LABEL_SIZE, REGULAR, Style, layout};
use kition_kit::kdm::ObjectId;
use kition_kit::session::BlobSource;
use serde_json::{Value, json};

use crate::{BoardDocument, RasterKey};

/// The most texts one `textMetrics` request may measure.
pub const MAX_TEXTS: usize = 500;

/// Registers a font file's faces for the engine's text (standard E9): the editor gives a Latin face
/// first, then Droid Sans Fallback for Chinese and Japanese. Answers how many faces were added.
pub fn add_font(bytes: &[u8]) -> usize {
    kition_board_text::add_font(bytes.to_vec())
}

impl BoardDocument {
    /// The document frame of the shown page.
    pub fn area(&self) -> Area {
        let key = self.raster_key();
        if let Ok(cache) = self.area.lock()
            && let Some((at, area)) = cache.as_ref()
            && *at == key
        {
            return *area;
        }
        let fresh = area(self.session.model(), self.active_page());
        if let Ok(mut cache) = self.area.lock() {
            *cache = Some((key, fresh));
        }
        fresh
    }

    /// What the shown page's raster and frame are drawn from.
    fn raster_key(&self) -> RasterKey {
        RasterKey {
            revision: self.revision,
            page: self.active_page().map(|page| page.id.clone()),
            fonts: kition_board_text::font_epoch(),
            blobs: self.blobs.len(),
        }
    }

    /// The raster of the shown page, cached until something it shows changes.
    pub(crate) fn raster(&self) -> Result<Arc<Raster>, String> {
        let key = self.raster_key();
        if let Ok(cache) = self.raster.lock()
            && let Some((at, raster)) = cache.as_ref()
            && *at == key
        {
            return Ok(raster.clone());
        }
        let raster = Arc::new(Raster::build(self.session.model(), self.active_page(), &self.blobs)?);
        if let Ok(mut cache) = self.raster.lock() {
            *cache = Some((key, raster.clone()));
        }
        Ok(raster)
    }

    /// The shown page's raster in `scheme`: the light one is [`Self::raster`]; the dark one is kept
    /// beside it, so the minimap and the previews do not draw each other's.
    fn raster_in(&self, scheme: Scheme) -> Result<Arc<Raster>, String> {
        if scheme == Scheme::Light {
            return self.raster();
        }
        let key = self.raster_key();
        if let Ok(cache) = self.dark_raster.lock()
            && let Some((at, raster)) = cache.as_ref()
            && *at == key
        {
            return Ok(raster.clone());
        }
        let raster = Arc::new(Raster::build_in(scheme, self.session.model(), self.active_page(), &self.blobs)?);
        if let Ok(mut cache) = self.dark_raster.lock() {
            *cache = Some((key, raster.clone()));
        }
        Ok(raster)
    }

    /// `renderScheme(scheme, x, y, scale, width, height)`: the contract's `render` in the host's
    /// color scheme (`light` or `dark`), for the editor's minimap. The same pixels' coverage as
    /// `render`; the tokens and the paper in the scheme's colours (`kition_board_scene::style`).
    pub fn render_scheme(&self, scheme: &str, x: f64, y: f64, scale: f64, width: u32, height: u32) -> Result<Vec<u8>, String> {
        let scheme = Scheme::parse(scheme).ok_or_else(|| format!("invalid-params: the scheme is light or dark, not `{scheme}`"))?;
        self.raster_in(scheme)?.render(x, y, scale, width, height)
    }

    /// `elementSvg(request)`: see `kition_board_scene::cache::Revisions::answer`.
    pub fn element_svg(&self, request: &str) -> Result<String, String> {
        let request: Value = serde_json::from_str(request).map_err(|error| format!("invalid-params: the request is not JSON: {error}"))?;
        let mut revisions = self.revisions.lock().map_err(|_| "the fragment revisions are unavailable".to_owned())?;
        let answer = revisions.answer(self.session.model(), &request, &self.blobs)?;
        Ok(answer.to_string())
    }

    /// `textMetrics({ texts: [{ text, size?, weight?, family?, maxWidth? }] })` →
    /// `{ results: [{ lines: [{ text, width }], width, height, lineHeight, css }], fontEpoch }`: the
    /// engine's layout, in board units; `css` is the `font-family` list the engine's SVG names for
    /// that family (the editor's box for a new text uses it).
    pub fn text_metrics(&self, request: &str) -> Result<String, String> {
        let request: Value = serde_json::from_str(request).map_err(|error| format!("invalid-params: the request is not JSON: {error}"))?;
        let texts = request.get("texts").and_then(Value::as_array).ok_or("invalid-params: `texts` must be a list")?;
        if texts.len() > MAX_TEXTS {
            return Err(format!("invalid-params: at most {MAX_TEXTS} texts per request"));
        }
        let mut results = Vec::with_capacity(texts.len());
        for item in texts {
            let text = item.get("text").and_then(Value::as_str).ok_or("invalid-params: every entry needs `text`")?;
            let weight = item.get("weight").and_then(Value::as_u64).map_or(REGULAR, |weight| weight.clamp(1, 1000) as u16);
            let style = Style::new(
                item.get("size").and_then(Value::as_f64).unwrap_or(LABEL_SIZE),
                weight,
                Family::parse(item.get("family").and_then(Value::as_str)),
            );
            let lines = layout(text, style, item.get("maxWidth").and_then(Value::as_f64));
            let width = lines.iter().map(|line| line.width).fold(0.0, f64::max);
            results.push(json!({
                "lines": lines.iter().map(|line| json!({ "text": line.text, "width": line.width })).collect::<Vec<_>>(),
                "width": width,
                "height": lines.len().max(1) as f64 * style.line_height(),
                "lineHeight": style.line_height(),
                "css": kition_board_text::css_family(style.family),
            }));
        }
        Ok(json!({ "results": results, "fontEpoch": kition_board_text::font_epoch() }).to_string())
    }

    /// `element.getStyle { ids } | { kind, shapeStyle? }` → `{ fields, style, palette }`: the style
    /// fields the elements (or a new element of `kind`) use, in `element.style` order; each one's
    /// value, the kind's default where it is not set, `null` where the elements differ; and the
    /// colour tokens with the colours they stand for (the style panel's swatches).
    pub fn style_of(&self, params: &Value) -> Result<Value, String> {
        let model = self.session.model();
        let described: Vec<(Kind, Option<String>, Option<&kition_board_kdm::Element>)> = match (params.get("ids"), params.get("kind")) {
            (Some(ids), None) => {
                let ids = ids.as_array().filter(|ids| !ids.is_empty()).ok_or("invalid-params: `ids` must be a non-empty list")?;
                let mut out = Vec::with_capacity(ids.len());
                for id in ids {
                    let id = id.as_str().ok_or("invalid-params: `ids` must hold ids (strings)")?;
                    let element = model.element(&ObjectId::from_string(id)).ok_or_else(|| format!("not-found: no element {id}"))?;
                    let shape_style = element.register("shapeStyle").and_then(Value::as_str).map(str::to_owned);
                    out.push((element.kind, shape_style, Some(element)));
                }
                out
            }
            (None, Some(kind)) => {
                let name = kind.as_str().ok_or("invalid-params: `kind` must be a kind")?;
                let kind = Kind::parse(name).ok_or_else(|| format!("invalid-params: unknown kind `{name}`"))?;
                let shape_style = params.get("shapeStyle").and_then(Value::as_str).map(str::to_owned);
                vec![(kind, shape_style, None)]
            }
            _ => return Err("invalid-params: give exactly one of `ids` and `kind`".into()),
        };
        let mut fields = Vec::new();
        let mut style = serde_json::Map::new();
        for field in STYLE_FIELDS.iter().copied().filter(|field| *field != "roundness") {
            let mut common: Option<Value> = None;
            let mut mixed = false;
            for (kind, shape_style, element) in &described {
                if !uses(*kind, field) {
                    continue;
                }
                let value = element
                    .and_then(|element| element.register(field).filter(|value| !value.is_null()).cloned())
                    .or_else(|| field_default(*kind, shape_style.as_deref(), field));
                match &common {
                    None => common = Some(value.unwrap_or(Value::Null)),
                    Some(seen) if Some(seen) != value.as_ref() => mixed = true,
                    Some(_) => {}
                }
            }
            if let Some(value) = common {
                fields.push(field);
                style.insert(field.to_owned(), if mixed { Value::Null } else { value });
            }
        }
        let palette: Vec<Value> = PALETTE.iter().map(|(token, stroke, fill)| json!({ "token": token, "stroke": stroke, "fill": fill })).collect();
        Ok(json!({ "fields": fields, "style": style, "palette": palette }))
    }

    /// `shape.previews { size? }` → `{ groups: [{ id, shapes: [{ shape, svg, outline? }] }] }`: the
    /// shape library's groups (`SHAPE_GROUPS`), each shape drawn by the renderer's own
    /// `shapes::outline` as a small SVG document, `size` × `size * 0.75` pixels (28 by default, 8 to
    /// 128), stroked in `currentColor` and not filled: the editor's library panel shows these
    /// pictures in its text colour, so it has no shape drawing of its own. `outline` is what a bound
    /// connector end meets ([`outline_json`]), so the page derives ends as the engine does (design
    /// note 1.4). A reading command: the board does not change.
    pub fn shape_previews(&self, params: &Value) -> Result<Value, String> {
        use kition_board_model::spec::{PREVIEW_SIZE, PREVIEW_SIZES};
        use kition_board_scene::shapes::{Paint, outline};
        use kition_board_scene::svg::num;
        let w = match params.get("size") {
            None => PREVIEW_SIZE,
            Some(size) => size
                .as_f64()
                .filter(|size| size.is_finite() && (PREVIEW_SIZES.0..=PREVIEW_SIZES.1).contains(size))
                .ok_or_else(|| format!("invalid-params: `size` must be a number from {} to {}", PREVIEW_SIZES.0, PREVIEW_SIZES.1))?,
        };
        let h = w * 0.75;
        // Room for half the stroke on every side.
        let pad = 1.0;
        let body = r#"fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round""#;
        let inner = r#"fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round" stroke-linecap="round""#;
        let look = Paint { body, inner, corner: None, sticky: false };
        let groups: Vec<Value> = SHAPE_GROUPS
            .iter()
            .map(|(group, shapes)| {
                let shapes: Vec<Value> = shapes
                    .iter()
                    .map(|shape| {
                        let mut svg = format!(
                            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="{} {} {} {}" aria-hidden="true">"#,
                            num(w),
                            num(h),
                            num(-pad),
                            num(-pad),
                            num(w + 2.0 * pad),
                            num(h + 2.0 * pad)
                        );
                        outline(&mut svg, shape, w, h, &look);
                        svg.push_str("</svg>");
                        let mut entry = serde_json::Map::new();
                        entry.insert("shape".into(), json!(shape));
                        entry.insert("svg".into(), json!(svg));
                        if let Some(outline) = outline_json(shape) {
                            entry.insert("outline".into(), outline);
                        }
                        Value::Object(entry)
                    })
                    .collect();
                json!({ "id": group, "shapes": shapes })
            })
            .collect();
        Ok(json!({ "groups": groups }))
    }

    /// `file.export { format: png | svg | pdf, page?, frame?, scale?, background?, fit? }` →
    /// `{ format, mime, extension, base64, width, height }`: the shown page (or `page`), or one
    /// frame and what is in it, drawn from the editor's scene. A reading command: the board does
    /// not change. It is the command `board_export` runs (`kit.json` `agent.export`, whose `data`
    /// is `base64`); it names no file, the host writes it.
    pub fn export_file(&self, params: &Value) -> Result<Value, String> {
        use base64::Engine as _;
        let (format, exported) = self.export_bytes(params)?;
        Ok(json!({
            "format": format.extension(),
            "mime": format.mime(),
            "extension": format.extension(),
            "base64": base64::engine::general_purpose::STANDARD.encode(&exported.bytes),
            "width": exported.width,
            "height": exported.height,
        }))
    }

    /// What `file.export` draws for `params`, as the file's bytes.
    pub(crate) fn export_bytes(&self, params: &Value) -> Result<(kition_board_scene::export::Format, kition_board_scene::export::Exported), String> {
        use kition_board_scene::export::{Format, Request, export};
        let model = self.session.model();
        let name = params.get("format").and_then(Value::as_str).ok_or("invalid-params: `format` must be png, svg or pdf")?;
        let format = Format::parse(name).ok_or_else(|| format!("invalid-params: `format` must be png, svg or pdf, not {name}"))?;
        let page = match params.get("page").and_then(Value::as_str) {
            Some(page) => model.page(&ObjectId::from_string(page)).ok_or_else(|| format!("not-found: no page {page}"))?,
            None => self.active_page().ok_or("not-found: the board has no page")?,
        };
        let frame = match params.get("frame").and_then(Value::as_str) {
            Some(id) => {
                let element = model.element(&ObjectId::from_string(id)).ok_or_else(|| format!("not-found: no element {id}"))?;
                if element.kind != Kind::Frame {
                    return Err(format!("invalid-params: {id} is a {}, not a frame", element.kind.as_str()));
                }
                if model.page_of(&element.id).map(|found| &found.id) != Some(&page.id) {
                    return Err("invalid-params: the frame is on another page".into());
                }
                Some(element)
            }
            None => None,
        };
        let scale = match params.get("scale") {
            None | Some(Value::Null) => 2.0,
            Some(value) => value
                .as_f64()
                .filter(|scale| scale.is_finite() && (0.1..=8.0).contains(scale))
                .ok_or("invalid-params: `scale` must be between 0.1 and 8")?,
        };
        let background = match params.get("background") {
            None | Some(Value::Null) => true,
            Some(value) => value.as_bool().ok_or("invalid-params: `background` must be true or false")?,
        };
        let fit = match params.get("fit") {
            None | Some(Value::Null) => false,
            Some(value) => value.as_bool().ok_or("invalid-params: `fit` must be true or false")?,
        };
        let exported = export(model, page, &Request { format, frame, scale, background, fit }, &self.blobs)?;
        Ok((format, exported))
    }

    /// A picture's bytes by blob hash (empty when the board does not hold them yet).
    pub fn blob(&self, hash: &str) -> Vec<u8> {
        self.blobs.get(hash).map(|bytes| bytes.to_vec()).unwrap_or_default()
    }
}

/// Board's additions to the exported module, for its embedded editor.
#[cfg(target_arch = "wasm32")]
mod web {
    use kition_kit::wasm_bindgen::prelude::*;

    use crate::__kition_kit_export::WebDocument;

    /// Adds a font file's faces for the engine's text; answers how many.
    #[wasm_bindgen(js_name = addFont, wasm_bindgen = kition_kit::wasm_bindgen)]
    pub fn add_font(bytes: &[u8]) -> u32 {
        super::add_font(bytes) as u32
    }

    #[wasm_bindgen(js_class = "BoardDocument", wasm_bindgen = kition_kit::wasm_bindgen)]
    impl WebDocument {
        #[wasm_bindgen(js_name = elementSvg)]
        pub fn element_svg(&self, request: &str) -> Result<String, JsError> {
            self.0.element_svg(request).map_err(|error| JsError::new(&error))
        }

        #[wasm_bindgen(js_name = renderScheme)]
        pub fn render_scheme(&self, scheme: &str, x: f64, y: f64, scale: f64, width: u32, height: u32) -> Result<Vec<u8>, JsError> {
            self.0.render_scheme(scheme, x, y, scale, width, height).map_err(|error| JsError::new(&error))
        }

        #[wasm_bindgen(js_name = textMetrics)]
        pub fn text_metrics(&self, request: &str) -> Result<String, JsError> {
            self.0.text_metrics(request).map_err(|error| JsError::new(&error))
        }

        pub fn blob(&self, hash: &str) -> Vec<u8> {
            self.0.blob(hash)
        }
    }
}

/// What a bound end of `shape` meets, for the page's mirror of `outline::outline_point`
/// (`web/src/board/outline.ts`): `"ellipse"`, the polygon's corners as `[fx, fy]` fractions of the
/// box, or `{ "slant": [fx, fy] }` for the parallelogram (inset `min(w * fx, h * fy)`); `None` when
/// it is the box.
fn outline_json(shape: &str) -> Option<Value> {
    use kition_board_kdm::outline::{Outline, outline_of};
    match outline_of(Some(shape)) {
        Outline::Box => None,
        Outline::Ellipse => Some(json!("ellipse")),
        Outline::Polygon(list) => Some(Value::Array(list.iter().map(|(fx, fy)| json!([fx, fy])).collect())),
        Outline::Slant(fx, fy) => Some(json!({ "slant": [fx, fy] })),
    }
}
