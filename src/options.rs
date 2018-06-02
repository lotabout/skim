use std::default::Default;
use clap::ArgMatches;

#[derive(Builder, Debug)]
#[builder(default)]
pub struct SkimOptions<'a> {
    pub bind: Vec<&'a str>,
    pub multi: bool,
    pub prompt: Option<&'a str>,
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
    pub replstr: Option<&'a str>,
    pub color: Option<&'a str>,
    pub margin: Option<&'a str>,
    pub min_height: Option<&'a str>,
    pub height: Option<&'a str>,
    pub preview: Option<&'a str>,
    pub preview_window: Option<&'a str>,
    pub reverse: bool,
    pub read0: bool,
    pub print0: bool,
    pub tabstop: Option<&'a str>,
    pub print_query: bool,
    pub print_cmd: bool,
    pub no_hscroll: bool,
}

impl<'a> SkimOptions<'a> {
    pub fn from_options(options: &'a ArgMatches) -> SkimOptions<'a> {
        let color = options.values_of("color").and_then(|vals| vals.last());
        let min_height = options.values_of("min-height").and_then(|vals| vals.last());
        let no_height = options.is_present("no-height");
        let height = if no_height {
            Some("100%")
        } else {
            options.values_of("height").and_then(|vals| vals.last())
        };
        let margin = options.values_of("margin").and_then(|vals| vals.last());
        let preview = options.values_of("preview").and_then(|vals| vals.last());
        let preview_window = options.values_of("preview-window").and_then(|vals| vals.last());

        let cmd = options.values_of("cmd").and_then(|vals| vals.last());
        let query = options.values_of("query").and_then(|vals| vals.last());
        let cmd_query = options.values_of("cmd-query").and_then(|vals| vals.last());
        let replstr = options.values_of("replstr").and_then(|vals| vals.last());
        let interactive =  options.is_present("interactive");
        let prompt = options.values_of("prompt").and_then(|vals| vals.last());
        let cmd_prompt = options.values_of("cmd-prompt").and_then(|vals| vals.last());

        let ansi = options.is_present("ansi");
        let delimiter = options.values_of("delimiter").and_then(|vals| vals.last());
        let with_nth = options.values_of("with-nth").and_then(|vals| vals.last());
        let nth = options.values_of("nth").and_then(|vals| vals.last());
        let read0 = options.is_present("read0");

        let bind = options.values_of("bind").map(|x| x.collect::<Vec<_>>()).unwrap_or_default();
        let expect = options.values_of("expect").map(|x| x.collect::<Vec<_>>().join(","));

        let multi = options.is_present("multi");
        let no_multi = options.is_present("no-multi");
        let reverse = options.is_present("reverse");
        let print0 = options.is_present("print0");
        let print_query = options.is_present("print-query");
        let print_cmd = options.is_present("print-cmd");
        let no_hscroll = options.is_present("no-hscroll");
        let tabstop = options.values_of("tabstop").and_then(|vals| vals.last());

        let tiebreak = options.values_of("tiebreak").map(|x| x.collect::<Vec<_>>().join(","));
        let tac = options.is_present("tac");
        let exact = options.is_present("exact");
        let regex = options.is_present("regex");

        SkimOptions {
            color,
            min_height,
            height,
            margin,
            preview,
            preview_window,
            cmd,
            query,
            cmd_query,
            replstr,
            interactive,
            prompt,
            cmd_prompt,
            ansi,
            delimiter,
            with_nth,
            nth,
            read0,
            bind,
            expect,
            multi: !no_multi && multi,
            reverse,
            print_query,
            print_cmd,
            print0,
            no_hscroll,
            tabstop,
            tiebreak,
            tac,
            exact,
            regex,
        }
    }
}


impl<'a> Default for SkimOptions<'a> {
    fn default() -> SkimOptions<'a> {
        SkimOptions {
            bind: Vec::new(),
            multi: false,
            prompt: Some("> "),
            cmd_prompt: Some("c> "),
            expect: None,
            tac: false,
            tiebreak: None,
            ansi: false,
            exact: false,
            cmd: None,
            interactive: false,
            query: None,
            cmd_query: None,
            regex: false,
            delimiter: None,
            nth: None,
            with_nth: None,
            replstr: Some("{}"),
            color: None,
            margin: Some("0,0,0,0"),
            min_height: Some("10"),
            height: Some("100%"),
            preview: None,
            preview_window: Some("right:50%"),
            reverse: false,
            read0: false,
            print0: false,
            tabstop: None,
            print_query: false,
            print_cmd: false,
            no_hscroll: false,
        }
    }
}
