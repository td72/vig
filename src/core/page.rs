use std::ffi::OsString;

#[derive(Debug)]
pub struct ExternalCommand {
    pub program: String,
    pub args: Vec<OsString>,
}

#[derive(Debug)]
pub enum PageAction {
    None,
    Suspend(ExternalCommand),
}
