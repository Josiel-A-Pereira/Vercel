/// One window the accessibility tree shows for a process: its title, and
/// whether the toolkit presents it as a prompt (a dialog or modal window).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::system) struct Frame {
    pub(in crate::system) title: String,
    pub(in crate::system) prompt: bool,
}
