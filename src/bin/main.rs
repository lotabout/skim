extern crate clap;
extern crate env_logger;
#[macro_use]
extern crate log;
extern crate shlex;
extern crate skim;
extern crate time;

use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::os::unix::io::AsRawFd;

use clap::{App, Arg, ArgMatches};
use nix::unistd::isatty;
use skim::prelude::*;

const VERSION: &str = env!("CARGO_PKG_VERSION");

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
    --algo=TYPE          Fuzzy matching algorithm:
                         [skim_v1|skim_v2|clangd] (default: skim_v2)
    --case [respect,ignore,smart] (default: smart)
                         case sensitive or not

  Interface
    -b, --bind KEYBINDS  comma seperated keybindings, in KEY:ACTION
                         such as 'ctrl-j:accept,ctrl-k:kill-line'
    -m, --multi          Enable Multiple Selection
    --no-multi           Disable Multiple Selection
    --no-mouse           Disable mouse events
    -c, --cmd ag         command to invoke dynamically
    -I replstr           replace `replstr` with the selected item
    -i, --interactive    Start skim in interactive(command) mode
    --color [BASE][,COLOR:ANSI]
                         change color theme
    --no-hscroll         Disable horizontal scroll

  Layout
    --layout=LAYOUT      Choose layout: [default|reverse|reverse-list]
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
    --inline-info        Display info next to query
    --header=STR         Display STR next to info
    --header-lines=N     The first N lines of the input are treated as header

  History
    --history=FILE       History file
    --history-size=N     Maximum number of query history entries (default: 1000)
    --cmd-history=FILE   command History file
    --cmd-history-size=N Maximum number of command history entries (default: 1000)

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
    --print-score        Print matching score in filter output (with --filter)
    -f, --filter=STR     Filter mode. Do not start interactive finder.

  Environment variables
    SKIM_DEFAULT_COMMAND Default command to use when input is tty
    SKIM_DEFAULT_OPTIONS Default options (e.g. '--ansi --regex')
                         You should not include other environment variables
                         (e.g. '-c \"$HOME/bin/ag\"')

  Reserved (not used for now)
    --extended
    --literal
    --cycle
    --hscroll-off=COL
    --filepath-word
    --jump-labels=CHARS
    --border
    --no-bold
    --history=FILE
    --history-size=N
    --sync
    --no-sort
    --select-1
    --exit-0
";

const DEFAULT_HISTORY_SIZE: usize = 1000;

//------------------------------------------------------------------------------
fn main() {
    use env_logger::fmt::Formatter;
    use env_logger::Builder;
    use log::{LevelFilter, Record};

    let format = |buf: &mut Formatter, record: &Record| {
        let t = time::now();
        writeln!(
            buf,
            "{},{:03} - {} - {}",
            time::strftime("%Y-%m-%d %H:%M:%S", &t).expect("main: time format error"),
            t.tm_nsec / 1_000_000,
            record.level(),
            record.args()
        )
    };

    let mut builder = Builder::new();
    builder.format(format).filter(None, LevelFilter::Info);

    if env::var("RUST_LOG").is_ok() {
        builder.parse_filters(&env::var("RUST_LOG").unwrap());
    }

    builder.try_init().expect("failed to initialize logger builder");

    match real_main() {
        Ok(exit_code) => std::process::exit(exit_code),
        Err(err) => {
            // if downstream pipe is closed, exit silently, see PR#279
            if err.kind() == std::io::ErrorKind::BrokenPipe {
                std::process::exit(0)
            }
            std::process::exit(2)
        }
    }
}

#[rustfmt::skip]
fn real_main() -> Result<i32, std::io::Error> {
    let mut stdout = std::io::stdout();

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
        .arg(Arg::with_name("cmd").long("cmd").short("c").multiple(true).takes_value(true))
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
        .arg(Arg::with_name("no-mouse").long("no-mouse").multiple(true))
        .arg(Arg::with_name("preview").long("preview").multiple(true).takes_value(true))
        .arg(Arg::with_name("preview-window").long("preview-window").multiple(true).takes_value(true).default_value("right:50%"))
        .arg(Arg::with_name("reverse").long("reverse").multiple(true))

        .arg(Arg::with_name("algorithm").long("algo").multiple(true).takes_value(true).default_value("skim_v2"))
        .arg(Arg::with_name("case").long("case").multiple(true).takes_value(true).default_value("smart"))
        .arg(Arg::with_name("literal").long("literal").multiple(true))
        .arg(Arg::with_name("cycle").long("cycle").multiple(true))
        .arg(Arg::with_name("no-hscroll").long("no-hscroll").multiple(true))
        .arg(Arg::with_name("hscroll-off").long("hscroll-off").multiple(true).takes_value(true).default_value("10"))
        .arg(Arg::with_name("filepath-word").long("filepath-word").multiple(true))
        .arg(Arg::with_name("jump-labels").long("jump-labels").multiple(true).takes_value(true).default_value("abcdefghijklmnopqrstuvwxyz"))
        .arg(Arg::with_name("border").long("border").multiple(true))
        .arg(Arg::with_name("inline-info").long("inline-info").multiple(true))
        .arg(Arg::with_name("header").long("header").multiple(true).takes_value(true).default_value(""))
        .arg(Arg::with_name("header-lines").long("header-lines").multiple(true).takes_value(true).default_value("0"))
        .arg(Arg::with_name("tabstop").long("tabstop").multiple(true).takes_value(true).default_value("8"))
        .arg(Arg::with_name("no-bold").long("no-bold").multiple(true))
        .arg(Arg::with_name("history").long("history").multiple(true).takes_value(true))
        .arg(Arg::with_name("cmd-history").long("cmd-history").multiple(true).takes_value(true))
        .arg(Arg::with_name("history-size").long("history-size").multiple(true).takes_value(true).default_value("1000"))
        .arg(Arg::with_name("cmd-history-size").long("cmd-history-size").multiple(true).takes_value(true).default_value("1000"))
        .arg(Arg::with_name("print-query").long("print-query").multiple(true))
        .arg(Arg::with_name("print-cmd").long("print-cmd").multiple(true))
        .arg(Arg::with_name("print-score").long("print-score").multiple(true))
        .arg(Arg::with_name("read0").long("read0").multiple(true))
        .arg(Arg::with_name("print0").long("print0").multiple(true))
        .arg(Arg::with_name("sync").long("sync").multiple(true))
        .arg(Arg::with_name("extended").long("extended").short("x").multiple(true))
        .arg(Arg::with_name("no-sort").long("no-sort").multiple(true))
        .arg(Arg::with_name("select-1").long("select-1").short("1").multiple(true))
        .arg(Arg::with_name("exit-0").long("exit-0").short("0").multiple(true))
        .arg(Arg::with_name("filter").long("filter").short("f").takes_value(true).multiple(true))
        .arg(Arg::with_name("layout").long("layout").multiple(true).takes_value(true).default_value("default"))
        .get_matches_from(args);

    if opts.is_present("help") {
        write!(stdout, "{}", USAGE)?;
        return Ok(0);
    }

    if opts.is_present("version") {
        writeln!(stdout, "{}", VERSION)?;
        return Ok(0);
    }

    //------------------------------------------------------------------------------
    // read in the history file
    let fz_query_histories = opts.values_of("history").and_then(|vals| vals.last());
    let cmd_query_histories = opts.values_of("cmd-history").and_then(|vals| vals.last());
    debug!("query_history_file: {:?}", fz_query_histories);
    debug!("cmd_history_file: {:?}", cmd_query_histories);
    let query_history = fz_query_histories.and_then(|filename| read_file_lines(filename).ok()).unwrap_or_else(|| vec![]);
    let cmd_history = cmd_query_histories.and_then(|filename| read_file_lines(filename).ok()).unwrap_or_else(|| vec![]);
    debug!("query_history: {:?}", query_history);
    debug!("cmd_history: {:?}", query_history);

    let mut options = parse_options(&opts);
    if fz_query_histories.is_some() || cmd_query_histories.is_some() {
        options.query_history = &query_history;
        options.cmd_history = &cmd_history;
        // bind ctrl-n and ctrl-p to handle history
        options.bind.insert(0, "ctrl-p:previous-history,ctrl-n:next-history");
    }

    let options = options;

    //------------------------------------------------------------------------------
    // read from pipe or command
    let stdin = std::io::stdin();
    let components_to_stop = Arc::new(AtomicUsize::new(0));
    let rx_item = match isatty(stdin.as_raw_fd()) {
        Ok(false) | Err(nix::Error::Sys(nix::errno::Errno::EINVAL)) => {
            let collector_option = CollectorOption::with_options(&options);
            let (rx_item, _) = read_and_collect_from_command(components_to_stop, CollectorInput::Pipe(Box::new(BufReader::new(stdin))), collector_option);
            Some(rx_item)
        }
        Ok(true) | Err(_) => None,
    };

    //------------------------------------------------------------------------------
    // filter mode
    if opts.is_present("filter") {
        return filter(&options, rx_item);
    }

    //------------------------------------------------------------------------------
    let output_ending = if options.print0 { "\0" } else { "\n" };

    let output = Skim::run_with(&options, rx_item);
    if output.is_none() {
        return Ok(130);
    }

    //------------------------------------------------------------------------------
    // output
    let output = output.unwrap();

    // output query
    if options.print_query {
        write!(stdout, "{}{}", output.query, output_ending)?;
    }

    if options.print_cmd {
        write!(stdout, "{}{}", output.cmd, output_ending)?;
    }

    if let Some(key) = output.accept_key {
        write!(stdout, "{}{}", key, output_ending)?;
    }

    for item in output.selected_items.iter() {
        write!(stdout, "{}{}", item.output(), output_ending)?;
    }

    //------------------------------------------------------------------------------
    // write the history with latest item
    if let Some(file) = fz_query_histories {
        let limit = opts.values_of("history-size").and_then(|vals| vals.last())
            .and_then(|size| size.parse::<usize>().ok())
            .unwrap_or(DEFAULT_HISTORY_SIZE);
        write_history_to_file(&query_history, &output.query, limit, file)?;
    }

    if let Some(file) = cmd_query_histories {
        let limit = opts.values_of("cmd-history-size").and_then(|vals| vals.last())
            .and_then(|size| size.parse::<usize>().ok())
            .unwrap_or(DEFAULT_HISTORY_SIZE);
        write_history_to_file(&cmd_history, &output.cmd, limit, file)?;
    }

    Ok(if output.selected_items.is_empty() { 1 } else { 0 })
}

fn parse_options<'a>(options: &'a ArgMatches) -> SkimOptions<'a> {
    SkimOptionsBuilder::default()
        .color(options.values_of("color").and_then(|vals| vals.last()))
        .min_height(options.values_of("min-height").and_then(|vals| vals.last()))
        .no_height(options.is_present("no-height"))
        .height(options.values_of("height").and_then(|vals| vals.last()))
        .margin(options.values_of("margin").and_then(|vals| vals.last()))
        .preview(options.values_of("preview").and_then(|vals| vals.last()))
        .preview_window(options.values_of("preview-window").and_then(|vals| vals.last()))
        .cmd(options.values_of("cmd").and_then(|vals| vals.last()))
        .query(options.values_of("query").and_then(|vals| vals.last()))
        .cmd_query(options.values_of("cmd-query").and_then(|vals| vals.last()))
        .replstr(options.values_of("replstr").and_then(|vals| vals.last()))
        .interactive(options.is_present("interactive"))
        .prompt(options.values_of("prompt").and_then(|vals| vals.last()))
        .cmd_prompt(options.values_of("cmd-prompt").and_then(|vals| vals.last()))
        .ansi(options.is_present("ansi"))
        .delimiter(options.values_of("delimiter").and_then(|vals| vals.last()))
        .with_nth(options.values_of("with-nth").and_then(|vals| vals.last()))
        .nth(options.values_of("nth").and_then(|vals| vals.last()))
        .read0(options.is_present("read0"))
        .bind(
            options
                .values_of("bind")
                .map(|x| x.collect::<Vec<_>>())
                .unwrap_or_default(),
        )
        .expect(options.values_of("expect").map(|x| x.collect::<Vec<_>>().join(",")))
        .multi(if options.is_present("no-multi") {
            false
        } else {
            options.is_present("multi")
        })
        .layout(options.values_of("layout").and_then(|vals| vals.last()).unwrap_or(""))
        .reverse(options.is_present("reverse"))
        .print0(options.is_present("print0"))
        .print_query(options.is_present("print-query"))
        .print_cmd(options.is_present("print-cmd"))
        .print_score(options.is_present("print-score"))
        .no_hscroll(options.is_present("no-hscroll"))
        .no_mouse(options.is_present("no-mouse"))
        .tabstop(options.values_of("tabstop").and_then(|vals| vals.last()))
        .tiebreak(options.values_of("tiebreak").map(|x| x.collect::<Vec<_>>().join(",")))
        .tac(options.is_present("tac"))
        .exact(options.is_present("exact"))
        .regex(options.is_present("regex"))
        .inline_info(options.is_present("inline-info"))
        .header(options.values_of("header").and_then(|vals| vals.last()))
        .header_lines(
            options
                .values_of("header-lines")
                .and_then(|vals| vals.last())
                .map(|s| s.parse::<usize>().unwrap_or(0))
                .unwrap_or(0),
        )
        .layout(options.values_of("layout").and_then(|vals| vals.last()).unwrap_or(""))
        .filter(options.values_of("filter").and_then(|vals| vals.last()).unwrap_or(""))
        .algorithm(FuzzyAlgorithm::of(
            options.values_of("algorithm").and_then(|vals| vals.last()).unwrap(),
        ))
        .case(match options.value_of("case") {
            Some("smart") => CaseMatching::Smart,
            Some("ignore") => CaseMatching::Ignore,
            _ => CaseMatching::Respect,
        })
        .build()
        .unwrap()
}

fn read_file_lines(filename: &str) -> Result<Vec<String>, std::io::Error> {
    let file = File::open(filename)?;
    let ret = BufReader::new(file).lines().collect();
    debug!("file content: {:?}", ret);
    ret
}

fn write_history_to_file(
    orig_history: &[String],
    latest: &str,
    limit: usize,
    filename: &str,
) -> Result<(), std::io::Error> {
    let additional_lines = if latest.trim().is_empty() { 0 } else { 1 };
    let start_index = if orig_history.len() + additional_lines > limit {
        orig_history.len() + additional_lines - limit
    } else {
        0
    };

    let mut history = orig_history[start_index..].to_vec();
    history.push(latest.to_string());

    let file = File::create(filename)?;
    let mut file = BufWriter::new(file);
    file.write_all(history.join("\n").as_bytes())?;
    Ok(())
}

pub fn filter(options: &SkimOptions, source: Option<SkimItemReceiver>) -> Result<i32, std::io::Error> {
    let mut stdout = std::io::stdout();

    let output_ending = if options.print0 { "\0" } else { "\n" };
    let query = options.filter;
    let default_command = match env::var("SKIM_DEFAULT_COMMAND").as_ref().map(String::as_ref) {
        Ok("") | Err(_) => "find .".to_owned(),
        Ok(val) => val.to_owned(),
    };
    let cmd = options.cmd.unwrap_or(&default_command);

    // output query
    if options.print_query {
        write!(stdout, "{}{}", query, output_ending)?;
    }

    if options.print_cmd {
        write!(stdout, "{}{}", cmd, output_ending)?;
    }

    //------------------------------------------------------------------------------
    // matcher
    let engine_factory: Box<dyn MatchEngineFactory> = if options.regex {
        Box::new(RegexEngineFactory::new())
    } else {
        let fuzzy_engine_factory = ExactOrFuzzyEngineFactory::builder()
            .fuzzy_algorithm(options.algorithm)
            .exact_mode(options.exact)
            .build();
        Box::new(AndOrEngineFactory::new(fuzzy_engine_factory))
    };

    let engine = engine_factory.create_engine_with_case(query, options.case);

    //------------------------------------------------------------------------------
    // start
    let item_index = AtomicUsize::new(0);
    let components_to_stop = Arc::new(AtomicUsize::new(0));
    let collector_option = CollectorOption::with_options(&options);

    let stream_of_item = source.unwrap_or_else(|| {
        let collector_input = CollectorInput::Command(cmd.to_string());
        let (ret, _control) = read_and_collect_from_command(components_to_stop, collector_input, collector_option);
        ret
    });

    let mut num_matched = 0;
    stream_of_item
        .into_iter()
        .map(|item| ItemWrapper::new(item, (0, item_index.fetch_add(0, Ordering::SeqCst))))
        .filter_map(|wrapped| engine.match_item(Arc::new(wrapped)))
        .try_for_each(|matched| {
            num_matched += 1;
            if options.print_score {
                writeln!(stdout, "{}\t{}", -matched.rank.score, matched.item.output())
            } else {
                writeln!(stdout, "{}", matched.item.output())
            }
        })?;

    Ok(if num_matched == 0 { 1 } else { 0 })
}
