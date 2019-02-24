use clap::ArgMatches;
use std::default::Default;

#[derive(Debug)]
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
    pub inline_info: bool,
    pub header: Option<&'a str>,
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
        let interactive = options.is_present("interactive");
        let prompt = options.values_of("prompt").and_then(|vals| vals.last());
        let cmd_prompt = options.values_of("cmd-prompt").and_then(|vals| vals.last());

        let ansi = options.is_present("ansi");
        let delimiter = options.values_of("delimiter").and_then(|vals| vals.last());
        let with_nth = options.values_of("with-nth").and_then(|vals| vals.last());
        let nth = options.values_of("nth").and_then(|vals| vals.last());
        let read0 = options.is_present("read0");

        let bind = options
            .values_of("bind")
            .map(|x| x.collect::<Vec<_>>())
            .unwrap_or_default();
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
        let inline_info = options.is_present("inline-info");
        let header = options.values_of("header").and_then(|vals| vals.last());

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
            inline_info,
            header,
        }
    }

    pub fn bind(self, bind: Vec<&'a str>) -> Self {
        Self { bind, ..self }
    }
    pub fn multi(self, multi: bool) -> Self {
        Self { multi, ..self }
    }
    pub fn prompt(self, prompt: &'a str) -> Self {
        Self {
            prompt: Some(prompt),
            ..self
        }
    }
    pub fn cmd_prompt(self, cmd_prompt: &'a str) -> Self {
        Self {
            cmd_prompt: Some(cmd_prompt),
            ..self
        }
    }
    pub fn expect(self, expect: String) -> Self {
        Self {
            expect: Some(expect),
            ..self
        }
    }
    pub fn tac(self, tac: bool) -> Self {
        Self { tac, ..self }
    }
    pub fn tiebreak(self, tiebreak: String) -> Self {
        Self {
            tiebreak: Some(tiebreak),
            ..self
        }
    }
    pub fn ansi(self, ansi: bool) -> Self {
        Self { ansi, ..self }
    }
    pub fn exact(self, exact: bool) -> Self {
        Self { exact, ..self }
    }
    pub fn cmd(self, cmd: &'a str) -> Self {
        Self { cmd: Some(cmd), ..self }
    }
    pub fn interactive(self, interactive: bool) -> Self {
        Self { interactive, ..self }
    }
    pub fn query(self, query: &'a str) -> Self {
        Self {
            query: Some(query),
            ..self
        }
    }
    pub fn cmd_query(self, cmd_query: &'a str) -> Self {
        Self {
            cmd_query: Some(cmd_query),
            ..self
        }
    }
    pub fn regex(self, regex: bool) -> Self {
        Self { regex, ..self }
    }
    pub fn delimiter(self, delimiter: &'a str) -> Self {
        Self {
            delimiter: Some(delimiter),
            ..self
        }
    }
    pub fn nth(self, nth: &'a str) -> Self {
        Self { nth: Some(nth), ..self }
    }
    pub fn with_nth(self, with_nth: &'a str) -> Self {
        Self {
            with_nth: Some(with_nth),
            ..self
        }
    }
    pub fn replstr(self, replstr: &'a str) -> Self {
        Self {
            replstr: Some(replstr),
            ..self
        }
    }
    pub fn color(self, color: &'a str) -> Self {
        Self {
            color: Some(color),
            ..self
        }
    }
    pub fn margin(self, margin: &'a str) -> Self {
        Self {
            margin: Some(margin),
            ..self
        }
    }
    pub fn min_height(self, min_height: &'a str) -> Self {
        Self {
            min_height: Some(min_height),
            ..self
        }
    }
    pub fn height(self, height: &'a str) -> Self {
        Self {
            height: Some(height),
            ..self
        }
    }
    pub fn preview(self, preview: &'a str) -> Self {
        Self {
            preview: Some(preview),
            ..self
        }
    }
    pub fn preview_window(self, preview_window: &'a str) -> Self {
        Self {
            preview_window: Some(preview_window),
            ..self
        }
    }
    pub fn reverse(self, reverse: bool) -> Self {
        Self { reverse, ..self }
    }
    pub fn read0(self, read0: bool) -> Self {
        Self { read0, ..self }
    }
    pub fn print0(self, print0: bool) -> Self {
        Self { print0, ..self }
    }
    pub fn tabstop(self, tabstop: &'a str) -> Self {
        Self {
            tabstop: Some(tabstop),
            ..self
        }
    }
    pub fn print_query(self, print_query: bool) -> Self {
        Self { print_query, ..self }
    }
    pub fn print_cmd(self, print_cmd: bool) -> Self {
        Self { print_cmd, ..self }
    }
    pub fn no_hscroll(self, no_hscroll: bool) -> Self {
        Self { no_hscroll, ..self }
    }
    pub fn inline_info(self, inline_info: bool) -> Self {
        Self { inline_info, ..self }
    }
    pub fn header(self, header: &'a str) -> Self {
        Self {
            header: Some(header),
            ..self
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
            inline_info: false,
            header: None,
        }
    }
}
