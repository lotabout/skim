use clap::Parser;
use derive_builder::Builder;
use log::debug;
use skim::prelude::*;
use std::{
    env,
    fs::File,
    io::{self, BufRead, BufReader, BufWriter, Write},
    path::Path,
    process::exit,
};

#[derive(Builder)]
pub struct BinOptions<'a> {
    filter: Option<&'a str>,
    output_ending: char,
    print_query: bool,
    print_cmd: bool,
}

fn main() {
    env_logger::builder().format_timestamp_nanos();

    match real_main() {
        Ok(exit_code) => exit(exit_code),
        Err(err) => {
            // if downstream pipe is closed, exit silently, see PR#279
            if err.kind() == io::ErrorKind::BrokenPipe {
                exit(0)
            }
            exit(2)
        }
    }
}

fn real_main() -> Result<i32, io::Error> {
    let mut stdout = io::stdout();

    let mut args = Vec::new();
    let mut cli_args = env::args();

    args.push(
        cli_args
            .next()
            .expect("there should be at least one arg: the application name"),
    );
    args.extend(
        env::var("SKIM_DEFAULT_OPTIONS")
            .ok()
            .and_then(|val| shlex::split(&val))
            .unwrap_or_default(),
    );
    for arg in cli_args {
        args.push(arg);
    }

    //------------------------------------------------------------------------------
    // parse options
    let cli = Cli::parse_from(args);

    //------------------------------------------------------------------------------
    let mut options = parse_options(&cli);

    //------------------------------------------------------------------------------
    // initialize collector
    let item_reader_option = SkimItemReaderOption::default()
        .ansi(cli.ansi)
        .delimiter(&cli.delimiter)
        .with_nth(&cli.with_nth)
        .nth(&cli.nth)
        .read0(cli.read0)
        .show_error(cli.show_cmd_error)
        .build();

    let cmd_collector = Rc::new(RefCell::new(SkimItemReader::new(item_reader_option)));
    options.cmd_collector = cmd_collector.clone();

    //------------------------------------------------------------------------------
    // read in the history file

    let query_history = try_read_file_lines(cli.history.as_deref());
    let cmd_history = try_read_file_lines(cli.cmd_history.as_deref());

    if cli.history.is_some() || cli.cmd_history.is_some() {
        options.query_history = &query_history;
        options.cmd_history = &cmd_history;

        // bind ctrl-n and ctrl-p to handle history
        options.bind.insert(0, "ctrl-p:previous-history,ctrl-n:next-history");
    }

    //------------------------------------------------------------------------------
    // handle pre-selection options
    let selector = DefaultSkimSelector::default()
        .first_n(cli.pre_select_n)
        .regex(&cli.pre_select_pat)
        .preset(
            cli.pre_select_items
                .as_ref()
                .map(|s| s.split('\n').map(str::to_owned).collect::<Vec<String>>())
                .unwrap_or_default(),
        )
        .preset(try_read_file_lines(cli.pre_select_file.as_deref()));
    options.selector = Some(Rc::new(selector));

    //------------------------------------------------------------------------------
    // mut  ==>  const
    let options = options;

    //------------------------------------------------------------------------------
    let bin_options = BinOptionsBuilder::default()
        .filter(cli.filter.as_deref())
        .print_query(cli.print_query)
        .print_cmd(cli.print_cmd)
        .output_ending(if cli.print0 { '\0' } else { '\n' })
        .build()
        .expect("");

    //------------------------------------------------------------------------------
    // read from pipe or command
    let rx_item =
        atty::isnt(atty::Stream::Stdin).then(|| cmd_collector.borrow().of_bufread(BufReader::new(io::stdin())));

    //------------------------------------------------------------------------------
    // filter mode
    if cli.filter.is_some() {
        return filter(&bin_options, &options, rx_item);
    }

    //------------------------------------------------------------------------------
    let output = Skim::run_with(&options, rx_item);
    if output.is_none() {
        return Ok(135); // error
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

    if !cli.expect.is_empty() {
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
    if let Some(file) = &cli.history {
        write_history_to_file(&query_history, &output.query, cli.history_size, file)?;
    }

    if let Some(file) = &cli.cmd_history {
        write_history_to_file(&cmd_history, &output.cmd, cli.cmd_history_size, file)?;
    }

    // nothing -> 1
    // something -> 0
    Ok(output.selected_items.is_empty().into())
}

// Because we are always using default value logic to deal with read lines,
// it would be better to return empty Vec when failing
fn try_read_file_lines(path: Option<&Path>) -> Vec<String> {
    path.and_then(|p| File::open(p).ok())
        .and_then(|f| {
            let lines: Result<Vec<String>, _> = BufReader::new(f).lines().collect();
            debug!("file content: {:?}", lines);
            lines.ok()
        })
        .unwrap_or_default()
}

fn to_strs(strings: &[String]) -> Vec<&str> {
    strings.iter().map(String::as_str).collect()
}

fn write_history_to_file(orig_history: &[String], latest: &str, limit: usize, path: &Path) -> Result<(), io::Error> {
    if let Some(true) = orig_history.last().map(|s| s == latest) {
        // no point of having at the end of the history 5x the same command...
        return Ok(());
    };

    let additional_lines = usize::from(!latest.trim().is_empty());
    let start_index = (orig_history.len() + additional_lines).saturating_sub(limit);

    let mut history = orig_history[start_index..].to_vec();
    history.push(latest.to_owned());

    let file = File::create(path)?;
    let mut file = BufWriter::new(file);
    file.write_all(history.join("\n").as_bytes())?;

    Ok(())
}

fn parse_options(options: &Cli) -> SkimOptions {
    SkimOptionsBuilder::default()
        .color(to_strs(&options.color))
        .min_height(Some(&options.min_height))
        .no_height(options.no_height)
        .height(Some(&options.height))
        .margin(to_strs(&options.margin))
        .preview(options.preview.as_deref())
        .preview_window(Some(&options.preview_window))
        .cmd(options.cmd.as_deref())
        .query(options.query.as_deref())
        .cmd_query(options.cmd_query.as_deref())
        .interactive(options.interactive)
        .prompt(Some(&options.prompt))
        .cmd_prompt(Some(&options.cmd_prompt))
        .bind(to_strs(&options.bind))
        .expect(to_strs(&options.expect))
        .multi(!options.no_multi && options.multi)
        .layout(options.layout)
        .reverse(options.reverse)
        .no_hscroll(options.no_hscroll)
        .no_mouse(options.no_mouse)
        .no_clear(options.no_clear)
        .tabstop(Some(options.tabstop))
        .tiebreak(options.tiebreak.clone())
        .tac(options.tac)
        .nosort(options.no_sort)
        .exact(options.exact)
        .regex(options.regex)
        .delimiter(&options.delimiter)
        .inline_info(options.inline_info)
        .header(options.header.as_deref())
        .header_lines(options.header_lines)
        .algorithm(options.algorithm)
        .case(options.case)
        .keep_right(options.keep_right)
        .skip_to_pattern(&options.skip_to_pattern)
        .select1(options.select_1)
        .exit0(options.exit_0)
        .sync(options.sync)
        .no_clear_if_empty(options.no_clear_if_empty)
        .no_clear_start(options.no_clear_start)
        .build()
        .unwrap()
}

pub fn filter(
    bin_option: &BinOptions,
    options: &SkimOptions,
    source: Option<SkimItemReceiver>,
) -> Result<i32, io::Error> {
    let mut stdout = io::stdout();

    let default_command = match env::var("SKIM_DEFAULT_COMMAND").as_deref() {
        Ok("") | Err(_) => "find .",
        Ok(val) => val,
    }
    .to_owned();

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

    // 0 -> 1
    // not 0 -> 0
    Ok((num_matched == 0).into())
}
