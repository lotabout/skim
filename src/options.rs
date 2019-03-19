use derive_builder::Builder;

#[derive(Debug, Builder)]
#[builder(build_fn(name = "final_build"))]
pub struct SkimOptions<'a> {
    pub bind: Vec<&'a str>,
    pub multi: bool,
    #[builder(default = "Some(\"> \")")]
    pub prompt: Option<&'a str>,
    #[builder(default = "Some(\"c> \")")]
    pub cmd_prompt: Option<&'a str>,
    pub expect: Option<String>,
    pub tac: bool,
    pub tiebreak: Option<String>,
    pub ansi: bool,
    pub exact: bool,
    pub cmd: Option<&'a str>,
    pub interactive: bool,
    pub query: Option<&'a str>,
    pub cmd_query: Option<&'a str>,
    pub regex: bool,
    pub delimiter: Option<&'a str>,
    pub nth: Option<&'a str>,
    pub with_nth: Option<&'a str>,
    #[builder(default = "Some(\"{}\")")]
    pub replstr: Option<&'a str>,
    pub color: Option<&'a str>,
    #[builder(default = "Some(\"0,0,0,0\")")]
    pub margin: Option<&'a str>,
    pub no_height: bool,
    #[builder(default = "Some(\"10\")")]
    pub min_height: Option<&'a str>,
    pub height: Option<&'a str>,
    pub preview: Option<&'a str>,
    #[builder(default = "Some(\"right:50%\")")]
    pub preview_window: Option<&'a str>,
    pub reverse: bool,
    pub read0: bool,
    pub print0: bool,
    pub tabstop: Option<&'a str>,
    pub print_query: bool,
    pub print_cmd: bool,
    pub no_hscroll: bool,
    pub inline_info: bool,
    pub header: Option<&'a str>,
    pub header_lines: usize,
    pub layout: &'a str,
}

impl<'a> SkimOptionsBuilder<'a> {
    pub fn build(&mut self) -> Result<SkimOptions<'a>, String> {
        if let Some(true) = self.no_height {
            self.height = Some(Some("100%"));
        }

        if let Some(true) = self.reverse {
            self.layout = Some("reverse");
        }

        self.final_build()
    }
}
