extern crate clap;
extern crate env_logger;
#[macro_use]
extern crate log;
extern crate atty;
extern crate shlex;
extern crate skim;
extern crate time;

use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};

use clap::parser::ValuesRef;
use clap::{crate_version, Arg, ArgAction, ArgMatches, Command};
use derive_builder::Builder;
use skim::prelude::*;

const USAGE: &str = r#"
Usage: sk [options]

  Options
    -h, --help           print this help menu
    --version            print out the current version of skim

  Search
    --tac                reverse the order of search result
    --no-sort            Do not sort the result
    -t, --tiebreak [score,begin,end,-score,length...]

                         comma seperated criteria
    -n, --nth 1,2..5     specify the fields to be matched
    --with-nth 1,2..5    specify the fields to be transformed
    -d, --delimiter \t  specify the delimiter(in REGEX) for fields
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
    -i, --interactive    Start skim in interactive(command) mode
    --color [BASE][,COLOR:ANSI]
                         change color theme
    --no-hscroll         Disable horizontal scroll
    --keep-right         Keep the right end of the line visible on overflow
    --skip-to-pattern    Line starts with the start of matched pattern
    --no-clear-if-empty  Do not clear previous items if command returns empty result
    --no-clear-start     Do not clear on start
    --show-cmd-error     Send command error message if command fails

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
                         [up|down|left|right][:SIZE[%]][:hidden][:+SCROLL[-OFFSET]]

  Scripting
    -q, --query ""       specify the initial query
    --cmd-query ""       specify the initial query for interactive mode
    --expect KEYS        comma seperated keys that can be used to complete skim
    --read0              Read input delimited by ASCII NUL(\0) characters
    --print0             Print output delimited by ASCII NUL(\0) characters
    --no-clear-start     Do not clear screen on start
    --no-clear           Do not clear screen on exit
    --print-query        Print query as the first line
    --print-cmd          Print command query as the first line (after --print-query)
    --print-score        Print matching score in filter output (with --filter)
    -1, --select-1       Automatically select the only match
    -0, --exit-0         Exit immediately when there's no match
    --sync               Synchronous search for multi-staged filtering
    --pre-select-n=NUM   Pre-select the first n items in multi-selection mode
    --pre-select-pat=REGEX
                         Pre-select the matched items in multi-selection mode
    --pre-select-items=$'item1\nitem2'
                         Pre-select the items separated by newline character
    --pre-select-file=FILENAME
                         Pre-select the items read from file

  Environment variables
    SKIM_DEFAULT_COMMAND Default command to use when input is tty
    SKIM_DEFAULT_OPTIONS Default options (e.g. '--ansi --regex')
                         You should not include other environment variables
                         (e.g. '-c "$HOME/bin/ag"')

  Removed
    -I replstr           replace `replstr` with the selected item

  Reserved (not used for now)
    --extended
    --literal
    --cycle
    --hscroll-off=COL
    --filepath-word
    --jump-labels=CHARS
    --border
    --no-bold
    --info
    --pointer
    --marker
    --phony
"#;

const DEFAULT_HISTORY_SIZE: usize = 1000;

//------------------------------------------------------------------------------
fn main() {
    env_logger::builder().format_timestamp_nanos().init();

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
    let opts = Command::new("sk")
        .author("Jinzhou Zhang<lotabout@gmail.com>")
        .version(crate_version!())
        .arg(Arg::new("bind").long("bind").short('b').action(ArgAction::Append))
        .arg(Arg::new("multi").long("multi").short('m').action(ArgAction::Count))
        .arg(Arg::new("no-multi").long("no-multi").action(ArgAction::Count))
        .arg(Arg::new("prompt").long("prompt").short('p').action(ArgAction::Append).default_value("> "))
        .arg(Arg::new("cmd-prompt").long("cmd-prompt").action(ArgAction::Append).default_value("c> "))
        .arg(Arg::new("expect").long("expect").action(ArgAction::Append))
        .arg(Arg::new("tac").long("tac").action(ArgAction::Count))
        .arg(Arg::new("tiebreak").long("tiebreak").short('t').action(ArgAction::Append))
        .arg(Arg::new("ansi").long("ansi").action(ArgAction::Count))
        .arg(Arg::new("exact").long("exact").short('e').action(ArgAction::Count))
        .arg(Arg::new("cmd").long("cmd").short('c').action(ArgAction::Append))
        .arg(Arg::new("interactive").long("interactive").short('i').action(ArgAction::Count))
        .arg(Arg::new("query").long("query").short('q').action(ArgAction::Append))
        .arg(Arg::new("cmd-query").long("cmd-query").action(ArgAction::Append))
        .arg(Arg::new("regex").long("regex").action(ArgAction::Count))
        .arg(Arg::new("delimiter").long("delimiter").short('d').action(ArgAction::Append))
        .arg(Arg::new("nth").long("nth").short('n').action(ArgAction::Append))
        .arg(Arg::new("with-nth").long("with-nth").action(ArgAction::Append))
        .arg(Arg::new("replstr").short('I').action(ArgAction::Append))
        .arg(Arg::new("color").long("color").action(ArgAction::Append))
        .arg(Arg::new("margin").long("margin").action(ArgAction::Append).default_value("0,0,0,0"))
        .arg(Arg::new("min-height").long("min-height").action(ArgAction::Append).default_value("10"))
        .arg(Arg::new("height").long("height").action(ArgAction::Append).default_value("100%"))
        .arg(Arg::new("no-height").long("no-height").action(ArgAction::Count))
        .arg(Arg::new("no-clear").long("no-clear").action(ArgAction::Count))
        .arg(Arg::new("no-clear-start").long("no-clear-start").action(ArgAction::Count))
        .arg(Arg::new("no-mouse").long("no-mouse").action(ArgAction::Count))
        .arg(Arg::new("preview").long("preview").action(ArgAction::Append))
        .arg(Arg::new("preview-window").long("preview-window").action(ArgAction::Append).default_value("right:50%"))
        .arg(Arg::new("reverse").long("reverse").action(ArgAction::Count))

        .arg(Arg::new("algorithm").long("algo").action(ArgAction::Append).default_value("skim_v2"))
        .arg(Arg::new("case").long("case").action(ArgAction::Append).default_value("smart"))
        .arg(Arg::new("literal").long("literal").action(ArgAction::Count))
        .arg(Arg::new("cycle").long("cycle").action(ArgAction::Count))
        .arg(Arg::new("no-hscroll").long("no-hscroll").action(ArgAction::Count))
        .arg(Arg::new("hscroll-off").long("hscroll-off").action(ArgAction::Append).default_value("10"))
        .arg(Arg::new("filepath-word").long("filepath-word").action(ArgAction::Count))
        .arg(Arg::new("jump-labels").long("jump-labels").action(ArgAction::Append).default_value("abcdefghijklmnopqrstuvwxyz"))
        .arg(Arg::new("border").long("border").action(ArgAction::Count))
        .arg(Arg::new("inline-info").long("inline-info").action(ArgAction::Count))
        .arg(Arg::new("header").long("header").action(ArgAction::Append).default_value(""))
        .arg(Arg::new("header-lines").long("header-lines").action(ArgAction::Append).default_value("0"))
        .arg(Arg::new("tabstop").long("tabstop").action(ArgAction::Append).default_value("8"))
        .arg(Arg::new("no-bold").long("no-bold").action(ArgAction::Count))
        .arg(Arg::new("history").long("history").action(ArgAction::Append))
        .arg(Arg::new("cmd-history").long("cmd-history").action(ArgAction::Append))
        .arg(Arg::new("history-size").long("history-size").action(ArgAction::Append).default_value("1000"))
        .arg(Arg::new("cmd-history-size").long("cmd-history-size").action(ArgAction::Append).default_value("1000"))
        .arg(Arg::new("print-query").long("print-query").action(ArgAction::Count))
        .arg(Arg::new("print-cmd").long("print-cmd").action(ArgAction::Count))
        .arg(Arg::new("print-score").long("print-score").action(ArgAction::Count))
        .arg(Arg::new("read0").long("read0").action(ArgAction::Count))
        .arg(Arg::new("print0").long("print0").action(ArgAction::Count))
        .arg(Arg::new("sync").long("sync").action(ArgAction::Count))
        .arg(Arg::new("extended").long("extended").short('x').action(ArgAction::Count))
        .arg(Arg::new("no-sort").long("no-sort").action(ArgAction::Count))
        .arg(Arg::new("select-1").long("select-1").short('1').action(ArgAction::Count))
        .arg(Arg::new("exit-0").long("exit-0").short('0').action(ArgAction::Count))
        .arg(Arg::new("filter").long("filter").short('f').action(ArgAction::Append))
        .arg(Arg::new("layout").long("layout").action(ArgAction::Append).default_value("default"))
        .arg(Arg::new("keep-right").long("keep-right").action(ArgAction::Count))
        .arg(Arg::new("skip-to-pattern").long("skip-to-pattern").action(ArgAction::Append).default_value(""))
        .arg(Arg::new("pre-select-n").long("pre-select-n").action(ArgAction::Append).default_value("0"))
        .arg(Arg::new("pre-select-pat").long("pre-select-pat").action(ArgAction::Append).default_value(""))
        .arg(Arg::new("pre-select-items").long("pre-select-items").action(ArgAction::Append))
        .arg(Arg::new("pre-select-file").long("pre-select-file").action(ArgAction::Append).default_value(""))
        .arg(Arg::new("no-clear-if-empty").long("no-clear-if-empty").action(ArgAction::Count))
        .arg(Arg::new("show-cmd-error").long("show-cmd-error").action(ArgAction::Count))
        .get_matches_from(args);

    if opts.contains_id("help") {
        write!(stdout, "{}", USAGE)?;
        return Ok(0);
    }

    //------------------------------------------------------------------------------
    let mut options = parse_options(&opts);

    let preview_window_joined = all_args(&opts, "preview-window", ":");
    options.preview_window = preview_window_joined.as_deref();

    //------------------------------------------------------------------------------
    // initialize collector
    let item_reader_option = SkimItemReaderOption::default()
        .ansi(has_flag(&opts, "ansi"))
        .delimiter(last_arg(&opts, "delimiter").unwrap_or(""))
        .with_nth(last_arg(&opts, "with-nth").unwrap_or(""))
        .nth(last_arg(&opts, "nth").unwrap_or(""))
        .read0(has_flag(&opts, "read0"))
        .show_error(has_flag(&opts, "show-cmd-error"))
        .build();

    let cmd_collector = Rc::new(RefCell::new(SkimItemReader::new(item_reader_option)));
    options.cmd_collector = cmd_collector.clone();

    //------------------------------------------------------------------------------
    // read in the history file
    let fz_query_histories = last_arg(&opts, "history");
    let cmd_query_histories = last_arg(&opts, "cmd-history");
    let query_history = fz_query_histories.and_then(|filename| read_file_lines(filename).ok()).unwrap_or_default();
    let cmd_history = cmd_query_histories.and_then(|filename| read_file_lines(filename).ok()).unwrap_or_default();

    if fz_query_histories.is_some() || cmd_query_histories.is_some() {
        options.query_history = &query_history;
        options.cmd_history = &cmd_history;
        // bind ctrl-n and ctrl-p to handle history
        options.bind.insert(0, "ctrl-p:previous-history,ctrl-n:next-history");
    }

    //------------------------------------------------------------------------------
    // handle pre-selection options
    let pre_select_n: Option<usize> = last_arg(&opts, "pre-select-n").and_then(|s| s.parse().ok());
    let pre_select_pat = last_arg(&opts, "pre-select-pat");
    let pre_select_items: Option<Vec<String>> = opts.get_many::<String>("pre-select-items").map(|vals| vals.flat_map(|m|m.split('\n')).map(|s|s.to_string()).collect());
    let pre_select_file = last_arg(&opts, "pre-select-file");

    if pre_select_n.is_some() || pre_select_pat.is_some() || pre_select_items.is_some() || pre_select_file.is_some() {
        let first_n = pre_select_n.unwrap_or(0);
        let pattern = pre_select_pat.unwrap_or("");
        let preset_items = pre_select_items.unwrap_or_default();
        let preset_file = pre_select_file.and_then(|filename| read_file_lines(filename).ok()).unwrap_or_default();

        let selector = DefaultSkimSelector::default()
            .first_n(first_n)
            .regex(pattern)
            .preset(preset_items)
            .preset(preset_file);
        options.selector = Some(Rc::new(selector));
    }

    let options = options;

    //------------------------------------------------------------------------------
    let bin_options = BinOptionsBuilder::default()
        .filter(last_arg(&opts, "filter"))
        .print_query(has_flag(&opts, "print-query"))
        .print_cmd(has_flag(&opts, "print-cmd"))
        .output_ending(if has_flag(&opts, "print0") { "\0" } else { "\n" })
        .build()
        .expect("");

    //------------------------------------------------------------------------------
    // read from pipe or command
    let rx_item = atty::isnt(atty::Stream::Stdin)
        .then(|| cmd_collector.borrow().of_bufread(BufReader::new(std::io::stdin())));

    //------------------------------------------------------------------------------
    // filter mode
    if opts.get_many::<String>("filter").is_some() {
        return filter(&bin_options, &options, rx_item);
    }

    //------------------------------------------------------------------------------
    let output = Skim::run_with(&options, rx_item);
    if output.is_none() { // error
        return Ok(135);
    }

    //------------------------------------------------------------------------------
    // output
    let output = output.unwrap();
    if output.is_abort {
        return Ok(130);
    }

    // output query
    if bin_options.print_query {
        write!(stdout, "{}{}", output.query, bin_options.output_ending)?;
    }

    if bin_options.print_cmd {
        write!(stdout, "{}{}", output.cmd, bin_options.output_ending)?;
    }

    if opts.get_many::<String>("expect").is_some() {
        match output.final_event {
            Event::EvActAccept(Some(accept_key)) => {
                write!(stdout, "{}{}", accept_key, bin_options.output_ending)?;
            }
            Event::EvActAccept(None) => {
                write!(stdout, "{}", bin_options.output_ending)?;
            }
            _ => {}
        }
    }

    for item in output.selected_items.iter() {
        write!(stdout, "{}{}", item.output(), bin_options.output_ending)?;
    }

    //------------------------------------------------------------------------------
    // write the history with latest item
    if let Some(file) = fz_query_histories {
        let limit = last_arg(&opts, "history-size")
            .and_then(|size: &str| size.parse::<usize>().ok())
            .unwrap_or(DEFAULT_HISTORY_SIZE);
        write_history_to_file(&query_history, &output.query, limit, file)?;
    }

    if let Some(file) = cmd_query_histories {
        let limit = last_arg(&opts, "cmd-history-size")
            .and_then(|size: &str| size.parse::<usize>().ok())
            .unwrap_or(DEFAULT_HISTORY_SIZE);
        write_history_to_file(&cmd_history, &output.cmd, limit, file)?;
    }

    Ok(if output.selected_items.is_empty() { 1 } else { 0 })
}

fn parse_options(options: &ArgMatches) -> SkimOptions<'_> {
    SkimOptionsBuilder::default()
        .color(last_arg(options, "color"))
        .min_height(last_arg(options, "min-height"))
        .no_height(has_flag(options, "no-height"))
        .height(last_arg(options, "height"))
        .margin(last_arg(options, "margin"))
        .preview(last_arg(options, "preview"))
        .cmd(last_arg(options, "cmd"))
        .query(last_arg(options, "query"))
        .cmd_query(last_arg(options, "cmd-query"))
        .interactive(has_flag(options, "interactive"))
        .prompt(last_arg(options, "prompt"))
        .cmd_prompt(last_arg(options, "cmd-prompt"))
        .bind(
            options
                .get_many::<String>("bind")
                .map(|x| x.map(String::as_str).collect::<Vec<_>>())
                .unwrap_or_default(),
        )
        .expect(all_args(options, "expect", ","))
        .multi(if has_flag(options, "no-multi") {
            false
        } else {
            has_flag(options, "multi")
        })
        .layout(last_arg(options, "layout").unwrap_or(""))
        .reverse(has_flag(options, "reverse"))
        .no_hscroll(has_flag(options, "no-hscroll"))
        .no_mouse(has_flag(options, "no-mouse"))
        .no_clear(has_flag(options, "no-clear"))
        .no_clear_start(has_flag(options, "no-clear-start"))
        .tabstop(last_arg(options, "tabstop"))
        .tiebreak(all_args(options, "tiebreak", ","))
        .tac(has_flag(options, "tac"))
        .nosort(has_flag(options, "no-sort"))
        .exact(has_flag(options, "exact"))
        .regex(has_flag(options, "regex"))
        .delimiter(last_arg(options, "delimiter"))
        .inline_info(has_flag(options, "inline-info"))
        .header(last_arg(options, "header"))
        .header_lines(
            last_arg(options, "header-lines")
                .map(|s| s.parse::<usize>().unwrap_or(0))
                .unwrap_or(0),
        )
        .layout(last_arg(options, "layout").unwrap_or(""))
        .algorithm(FuzzyAlgorithm::of(last_arg(options, "algorithm").unwrap()))
        .case(match last_arg(options, "case") {
            Some("smart") => CaseMatching::Smart,
            Some("ignore") => CaseMatching::Ignore,
            _ => CaseMatching::Respect,
        })
        .keep_right(has_flag(options, "keep-right"))
        .skip_to_pattern(last_arg(options, "skip-to-pattern").unwrap_or(""))
        .select1(has_flag(options, "select-1"))
        .exit0(has_flag(options, "exit-0"))
        .sync(has_flag(options, "sync"))
        .no_clear_if_empty(has_flag(options, "no-clear-if-empty"))
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
    if orig_history.last().map(|l| l.as_str()) == Some(latest) {
        // no point of having at the end of the history 5x the same command...
        return Ok(());
    }
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

#[derive(Builder)]
pub struct BinOptions<'a> {
    filter: Option<&'a str>,
    output_ending: &'a str,
    print_query: bool,
    print_cmd: bool,
}

pub fn filter(
    bin_option: &BinOptions,
    options: &SkimOptions,
    source: Option<SkimItemReceiver>,
) -> Result<i32, std::io::Error> {
    let mut stdout = std::io::stdout();

    let default_command = match env::var("SKIM_DEFAULT_COMMAND").as_ref().map(String::as_ref) {
        Ok("") | Err(_) => "find .".to_owned(),
        Ok(val) => val.to_owned(),
    };
    let query = bin_option.filter.unwrap_or("");
    let cmd = options.cmd.unwrap_or(&default_command);

    // output query
    if bin_option.print_query {
        write!(stdout, "{}{}", query, bin_option.output_ending)?;
    }

    if bin_option.print_cmd {
        write!(stdout, "{}{}", cmd, bin_option.output_ending)?;
    }

    //------------------------------------------------------------------------------
    // matcher
    let engine_factory: Box<dyn MatchEngineFactory> = if options.regex {
        Box::new(RegexEngineFactory::builder())
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
    let components_to_stop = Arc::new(AtomicUsize::new(0));

    let stream_of_item = source.unwrap_or_else(|| {
        let cmd_collector = options.cmd_collector.clone();
        let (ret, _control) = cmd_collector.borrow_mut().invoke(cmd, components_to_stop);
        ret
    });

    let mut num_matched = 0;
    stream_of_item
        .into_iter()
        .filter_map(|item| engine.match_item(item.clone()).map(|result| (item, result)))
        .try_for_each(|(item, _match_result)| {
            num_matched += 1;
            write!(stdout, "{}{}", item.output(), bin_option.output_ending)
        })?;

    Ok(if num_matched == 0 { 1 } else { 0 })
}

#[inline]
fn has_flag(options: &ArgMatches, id: &str) -> bool {
    options.get_count(id) > 0
}

#[inline]
fn last_arg<'a>(options: &'a ArgMatches, id: &str) -> Option<&'a str> {
    options
        .get_many::<String>(id)
        .and_then(ValuesRef::last)
        .map(String::as_str)
}

#[inline]
fn all_args(options: &ArgMatches, id: &str, sep: &str) -> Option<String> {
    options
        .get_many::<String>(id)
        .map(|ags| ags.map(String::as_str).collect::<Vec<_>>().join(sep))
}
