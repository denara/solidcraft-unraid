//! Presentation/projection state for an exact selected planar source placement.
use crate::{DocError, Document, FaceAnchor, ModelBody, ModelState, Result};
use std::borrow::Cow;

pub fn state_in_face_anchor_frame<'a>(doc: &Document, state: &'a ModelState, anchor: &FaceAnchor, author: u64) -> Result<Cow<'a, ModelState>> {
    let frame = anchor.author_frame(doc, author)?;
    let raw = state.body(&anchor.target.body).ok_or_else(|| DocError::Unknown(format!("anchored body `{}`", anchor.target.body)))?;
    let relative = anchor.source_to_author(doc, author, raw)?;
    let mut output = crate::state_in_frame_at(doc, state, author, frame).into_owned();
    let shape = if crate::is_identity(&relative) { raw.body.clone() } else { solidcraft_kernel::transform_matrix(&raw.body, relative)? };
    let mut body = ModelBody::new(raw.name.clone(), shape, raw.feature);
    body.component = raw.component;
    body.transferred = raw.transferred;
    body.source_name = raw.source_name.clone();
    body.source_transform = crate::mat_mul(&relative, &raw.source_transform);
    body.imported = raw.imported.clone();
    body.names = crate::naming::NameCell::same(raw);
    let slot = output.bodies.iter_mut().find(|body| body.name == raw.name).ok_or_else(|| DocError::Unknown("anchored body frame".into()))?;
    *slot = body;
    Ok(Cow::Owned(output))
}
