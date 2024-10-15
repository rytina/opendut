#[derive(Debug, Clone, Copy)]
pub enum Crate {
    MdbookPlantuml,
}
impl Crate {
    pub fn ident(&self) -> &'static str {
        match self {
            Crate::MdbookPlantuml => "mdbook-plantuml",
        }
    }
    pub fn install_command_args(&self) -> &[&'static str] {
        &[]
    }
}
