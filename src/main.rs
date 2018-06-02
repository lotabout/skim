extern crate clap;
extern crate env_logger;
extern crate log;
extern crate shlex;
extern crate skim;
extern crate time;

use clap::{App, Arg};
use skim::{Skim, SkimOptions};
use std::env;

const VERSION: &str = "0.3.2";

const USAGE: &str = "
Usage: sk [options]

  Options
    -h, --help           print this help menu
    --version            print out the current version of skim

  Search
    --tac                reverse the order of input
    -t, --tiebreak [score,index,begin,end,-score,...]
                         comma seperated criteria
    -n, --nth 1,2..5     specify the fields to be matched
    --with-nth 1,2..5    specify the fields to be transformed
    -d, --delimiter \\t  specify the delimiter(in REGEX) for fields
    -e, --exact          start skim in exact mode
    --regex              use regex instead of fuzzy match

  Interface
    -b, --bind KEYBINDS  comma seperated keybindings, in KEY:ACTION
                         such as 'ctrl-j:accept,ctrl-k:kill-line'
    -m, --multi          Enable Multiple Selection
    --no-multi           Disable Multiple Selection
    -c, --cmd ag         command to invoke dynamically
    -I replstr           replace `replstr` with the selected item
    -i, --interactive    Start skim in interactive(command) mode
    --color [BASE][,COLOR:ANSI]
                         change color theme
    --no-hscroll         Disable horizontal scroll

  Layout
    --reverse            Reverse orientation
    --height=HEIGHT      Height of skim's window (--height 40%)
    --no-height          Disable height feature
    --min-height=HEIGHT  Minimum height when --height is given by percent
                         (default: 10)
    --margin=MARGIN      Screen Margin (TRBL / TB,RL / T,RL,B / T,R,B,L)
                         e.g. (sk --margin 1,10%)
    -p, --prompt '> '    prompt string for query mode
    --cmd-prompt '> '    prompt string for command mode

  Display
    --ansi               parse ANSI color codes for input strings
    --tabstop=SPACES     Number of spaces for a tab character (default: 8)

  Preview
    --preview=COMMAND    command to preview current highlighted line ({})
                         We can specify the fields. e.g. ({1}, {..3}, {0..})
    --preview-window=OPT Preview window layout (default: right:50%)
                         [up|down|left|right][:SIZE[%]][:hidden]

  Scripting
    -q, --query \"\"       specify the initial query
    --cmd-query \"\"       specify the initial query for interactive mode
    --expect KEYS        comma seperated keys that can be used to complete skim
    --read0              Read input delimited by ASCII NUL(\\0) characters
    --print0             Print output delimited by ASCII NUL(\\0) characters
    --print-query        Print query as the first line
    --print-cmd          Print command query as the first line (after --print-query)

  Environment variables
    SKIM_DEFAULT_COMMAND Default command to use when input is tty
    SKIM_DEFAULT_OPTIONS Default options (e.g. '--ansi --regex')
                         You should not include other environment variables
                         (e.g. '-c \"$HOME/bin/ag\"')

  Reserved (not used for now)
    --extended
    --algo=TYPE
    --literal
    --no-mouse
    --cycle
    --hscroll-off=COL
    --filepath-word
    --jump-labels=CHARS
    --border
    --inline-info
    --header=STR
    --header-lines=N
    --no-bold
    --history=FILE
    --history-size=N
    --sync
    --no-sort
    --select-1
    --exit-0
    --filter
";

fn main() {
    use env_logger::LogBuilder;
    use log::{LogLevelFilter, LogRecord};

    let format = |record: &LogRecord| {
        let t = time::now();
        format!(
            "{},{:03} - {} - {}",
            time::strftime("%Y-%m-%d %H:%M:%S", &t).expect("main: time format error"),
            t.tm_nsec / 1_000_000,
            record.level(),
            record.args()
        )
    };

    let mut builder = LogBuilder::new();
    builder.format(format).filter(None, LogLevelFilter::Info);

    if env::var("RUST_LOG").is_ok() {
        builder.parse(&env::var("RUST_LOG").unwrap());
    }

    builder.init().expect("failed to initialize logger builder");

    let exit_code = real_main();
    std::process::exit(exit_code);
}

#[cfg_attr(rustfmt, rustfmt_skip)]
fn real_main() -> i32 {
    let mut args = Vec::new();

    args.push(env::args().next().expect("there should be at least one arg: the application name"));
    args.extend(env::var("SKIM_DEFAULT_OPTIONS")
                .ok()
                .and_then(|val| shlex::split(&val))
                .unwrap_or_default());
    for arg in env::args().skip(1) {
        args.push(arg);
    }


    //------------------------------------------------------------------------------
    // parse options
    let opts = App::new("sk")
        .author("Jinzhou Zhang<lotabout@gmail.com")
        .arg(Arg::with_name("help").long("help").short("h"))
        .arg(Arg::with_name("version").long("version").short("v"))
        .arg(Arg::with_name("bind").long("bind").short("b").multiple(true).takes_value(true))
        .arg(Arg::with_name("multi").long("multi").short("m").multiple(true))
        .arg(Arg::with_name("no-multi").long("no-multi").multiple(true))
        .arg(Arg::with_name("prompt").long("prompt").short("p").multiple(true).takes_value(true).default_value("> "))
        .arg(Arg::with_name("cmd-prompt").long("cmd-prompt").multiple(true).takes_value(true).default_value("c> "))
        .arg(Arg::with_name("expect").long("expect").multiple(true).takes_value(true))
        .arg(Arg::with_name("tac").long("tac").multiple(true))
        .arg(Arg::with_name("tiebreak").long("tiebreak").short("t").multiple(true).takes_value(true))
        .arg(Arg::with_name("ansi").long("ansi").multiple(true))
        .arg(Arg::with_name("exact").long("exact").short("e").multiple(true))
        .arg(Arg::with_name("cmd").long("cmd").short("cmd").multiple(true).takes_value(true))
        .arg(Arg::with_name("interactive").long("interactive").short("i").multiple(true))
        .arg(Arg::with_name("query").long("query").short("q").multiple(true).takes_value(true))
        .arg(Arg::with_name("cmd-query").long("cmd-query").multiple(true).takes_value(true))
        .arg(Arg::with_name("regex").long("regex").multiple(true))
        .arg(Arg::with_name("delimiter").long("delimiter").short("d").multiple(true).takes_value(true))
        .arg(Arg::with_name("nth").long("nth").short("n").multiple(true).takes_value(true))
        .arg(Arg::with_name("with-nth").long("with-nth").multiple(true).takes_value(true))
        .arg(Arg::with_name("replstr").short("I").multiple(true).takes_value(true))
        .arg(Arg::with_name("color").long("color").multiple(true).takes_value(true))
        .arg(Arg::with_name("margin").long("margin").multiple(true).takes_value(true).default_value("0,0,0,0"))
        .arg(Arg::with_name("min-height").long("min-height").multiple(true).takes_value(true).default_value("10"))
        .arg(Arg::with_name("height").long("height").multiple(true).takes_value(true).default_value("100%"))
        .arg(Arg::with_name("no-height").long("no-height").multiple(true))
        .arg(Arg::with_name("preview").long("preview").multiple(true).takes_value(true))
        .arg(Arg::with_name("preview-window").long("preview-window").multiple(true).takes_value(true).default_value("right:50%"))
        .arg(Arg::with_name("reverse").long("reverse").multiple(true))

        .arg(Arg::with_name("algorithm").long("algo").multiple(true).takes_value(true).default_value(""))
        .arg(Arg::with_name("literal").long("literal").multiple(true))
        .arg(Arg::with_name("no-mouse").long("no-mouse").multiple(true))
        .arg(Arg::with_name("cycle").long("cycle").multiple(true))
        .arg(Arg::with_name("no-hscroll").long("no-hscroll").multiple(true))
        .arg(Arg::with_name("hscroll-off").long("hscroll-off").multiple(true).takes_value(true).default_value("10"))
        .arg(Arg::with_name("filepath-word").long("filepath-word").multiple(true))
        .arg(Arg::with_name("jump-labels").long("jump-labels").multiple(true).takes_value(true).default_value("abcdefghijklmnopqrstuvwxyz"))
        .arg(Arg::with_name("border").long("border").multiple(true))
        .arg(Arg::with_name("inline-info").long("inline-info").multiple(true))
        .arg(Arg::with_name("header").long("header").multiple(true).takes_value(true).default_value(""))
        .arg(Arg::with_name("header-lines").long("header-lines").multiple(true).takes_value(true).default_value("1"))
        .arg(Arg::with_name("tabstop").long("tabstop").multiple(true).takes_value(true).default_value("8"))
        .arg(Arg::with_name("no-bold").long("no-bold").multiple(true))
        .arg(Arg::with_name("history").long("history").multiple(true).takes_value(true).default_value(""))
        .arg(Arg::with_name("history-size").long("history-size").multiple(true).takes_value(true).default_value("500"))
        .arg(Arg::with_name("print-query").long("print-query").multiple(true))
        .arg(Arg::with_name("print-cmd").long("print-cmd").multiple(true))
        .arg(Arg::with_name("read0").long("read0").multiple(true))
        .arg(Arg::with_name("print0").long("print0").multiple(true))
        .arg(Arg::with_name("sync").long("sync").multiple(true))
        .arg(Arg::with_name("extended").long("extended").short("x").multiple(true))
        .arg(Arg::with_name("no-sort").long("no-sort").multiple(true))
        .arg(Arg::with_name("select-1").long("select-1").short("1").multiple(true))
        .arg(Arg::with_name("exit-0").long("exit-0").short("0").multiple(true))
        .arg(Arg::with_name("filter").long("filter").short("f").multiple(true))
        .get_matches_from(args);

    if opts.is_present("help") {
        print!("{}", USAGE);
        return 0;
    }

    if opts.is_present("version") {
        println!("{}", VERSION);
        return 0;
    }

    let options = SkimOptions::from_options(&opts);
    let output_ending = if options.print0 {"\0"} else {"\n"};

    let output = Skim::run_with(&options, None);
    if let None = output {
        return 130;
    }

    let output = output.unwrap();

    // output query
    if options.print_query {
        print!("{}{}", output.query, output_ending);
    }

    if options.print_cmd {
        print!("{}{}", output.cmd, output_ending);
    }

    output.accept_key.map(|key| {
        print!("{}{}", key, output_ending);
    });

    for item in output.selected_items.iter() {
        print!("{}{}", item.item.get_output_text(), output_ending);
    }

    if output.selected_items.len() > 0 {0} else {1}
}
