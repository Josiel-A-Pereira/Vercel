/// What the Wayland output protocols reported about one output, before it is
/// turned into a logical monitor. `wl_output` supplies the mode, transform
/// and integer scale; `xdg_output` supplies the logical position and size.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct OutputRecord {
    pub(crate) global: u32,
    pub(crate) output_name: Option<String>,
    pub(crate) logical_name: Option<String>,
    pub(crate) position: Option<(i32, i32)>,
    pub(crate) size: Option<(i32, i32)>,
    pub(crate) mode: Option<(i32, i32)>,
    pub(crate) transform: u32,
    pub(crate) integer_scale: i32,
}
