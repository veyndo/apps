//! Bounded twelve-column form geometry, presentation flags and tab order shared by every native app surface.
use super::*;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Geometry {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}
pub(super) fn validate(b: &native_views::NativeBlock) -> Result<()> {
    if b.geometry.as_ref().is_some_and(|g| {
        g.x >= 12 || g.y > 200 || g.w == 0 || g.w > 12 || g.x + g.w > 12 || g.h == 0 || g.h > 40
    }) || b.tab_order.is_some_and(|n| n > 1000)
        || b.tooltip.values().any(|s| s.len() > 300)
    {
        return Err(bad("Invalid native form geometry or presentation"));
    }
    Ok(())
}
