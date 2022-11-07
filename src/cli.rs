use crate::engine::fuzzy::FuzzyAlgorithm;
use crate::item::RankCriteria;
use crate::{CaseMatching, Layout};
use clap::Parser;
use std::path::PathBuf;

const USAGE: &str = r#"
Fuzzy Finder in rust!

Zhang Jinzhou <lotabout@gmail.com>

Usage: sk [OPTIONS]

Options
  -h, --help                        Print help information
  -V, --version                     Print version information

Search
      --tac                         Reverse the order of search result
      --no-sort                     Do not sort the result
  -t, --tiebreak=[score, -score, begin, -begin, end, -end, length, -length]
                                    Comma seperated criteria
  -n, --nth=1,2..5                  Specify the fields to be matched
      --with-nth=1,2..5             Specify the fields to be transformed
  -d, --delimiter='[\t\n ]+'        Specify the delimiter (in REGEX) for fields
  -e, --exact                       Start skim in exact mode
      --regex                       Use regex instead of fuzzy match
      --algo=[skim_v1|skim_v2|clangd] (default: skim_v2)
                                    Fuzzy matching pub algorithm
      --case=[respect|ignore|smart] (default: smart)
                                    Case sensitive or not

Interface
  -b, --bind=<KEYBINDS>             Comma seperated keybindings, in pub KEY:ACTION,
                                    such as 'ctrl-pub j:accept,ctrl-k:kill-line'
  -m, --multi                       Enable Multiple Selection
      --no-multi                    Disable Multiple Selection
      --no-mouse                    Disable mouse events
  -c, --cmd=rg                      Command to invoke dynamically
  -i, --interactive                 Start skim in interactive (command) mode
      --color=[BASE][,COLOR:ANSI]
                                    Change color theme
      --no-hscroll                  Disable horizontal scroll
      --keep-right                  Keep the right end of the line visible on overflow
      --skip-to-pattern=""          Line starts with the start of matched pattern
      --no-clear-if-empty           Do not clear previous items if command returns empty result
      --no-clear-start              Do not clear on start
      --show-cmd-error              Send command error message if command fails

Layout
      --layout=[default|reverse|reverse-list]
                                    Choose layout
      --reverse                     Shortcut for `layout='reverse'`
      --height=100%                 Height of skim's window
      --no-height                   Disable height feature
      --min-height=10               Minimum height when --height is given by percent
      --margin=0,0,0,0              Screen Margin (TRBL / TB,RL / T,RL,B / T,R,B,L)
                                    e.g. `sk --margin 1,10%`
  -p, --prompt='> '                 Prompt string for query mode
      --cmd-prompt='c> '            Prompt string for command mode

Display
      --ansi                        Parse ANSI color codes for input strings
      --tabstop=8                   Number of spaces for a tab character
      --header=<STR>                Display STR next to info
      --header-lines=0              The first N lines of the input are treated as header
      --inline-info                 Display info next to query

History
      --history=<FILE>              History file
      --history-size=1000           Maximum number of query history entries
      --cmd-history=<FILE>          Command History file
      --cmd-history-size=1000       Maximum number of command history entries

Preview
      --preview=<COMMAND>           Command to preview current highlighted line ({})
                                    We can specify the fields. e.g. ({1}, {..3}, {0..})
      --preview-window=right:50%    Preview window layout
                                    [up|down|left|right][:SIZE[%]][:hidden][:+SCROLL[-OFFSET]]

Scripting
  -q, --query=""                    Specify the initial query
      --cmd-query=""                Specify the initial query for interactive mode
      --expect=<KEYS>               Comma seperated keys that can be used to complete skim
      --read0                       Read input delimited by ASCII NUL(\0) characters
      --print0                      Print output delimited by ASCII NUL(\0) characters
      --no-clear-start              Do not clear on start
      --no-clear                    Do not clear screen on exit
  -f, --filter <FILTER>             Filter mode, output the score and the item to stdout
      --print-query                 Print query as the first line
      --print-cmd                   Print command query as the first line (after --print-query)
      --print-score                 Print matching score in filter output (with --filter)
  -1, --select-1                    Automatically select the only match
  -0, --exit-0                      Exit immediately when there's no match
      --sync                        Synchronous search for multi-staged filtering
      --pre-select-n=0              Pre-select the first n items in multi-selection mode
      --pre-select-pat=""           Pre-select the matched items in multi-selection mode
      --pre-select-items=$'item1\nitem2'
                                    Pre-select the matched items in multi-selection mode
      --pre-select-file=<FILE>      Pre-select the items read from file

Removed
  -I <REPLSTR>                      Replace `replstr` with the selected item

Reserved (not used for now)
  -x, --extended
      --literal
      --cycle
      --hscroll-off=10
      --filepath-word
      --jump-labels='abcdefghijklmnopqrstuvwxyz'
      --border
      --no-bold

Environment variables
        SKIM_DEFAULT_COMMAND        Default command to use when input is tty
        SKIM_DEFAULT_OPTIONS        Default options (e.g. '--ansi --regex')
"#;

#[derive(Parser)]
#[command(name = "sk", version, override_help = USAGE)]
pub struct Cli {
    /* Search */
    /// Reverse the order of search result
    #[arg(long)]
    pub tac: bool,

    /// Do not sort the result
    #[arg(long)]
    pub no_sort: bool,

    /// Comma seperated criteria
    #[arg(short, long)]
    pub tiebreak: Vec<RankCriteria>,

    /// Specify the fields to be matched
    #[arg(short, long)]
    pub nth: Vec<String>,

    /// Specify the fields to be transformed
    #[arg(long)]
    pub with_nth: Vec<String>,

    /// Specify the delimiter (in REGEX) for fields
    #[arg(short, long, default_value = "")]
    pub delimiter: String,

    /// Start skim in exact mode
    #[arg(short, long)]
    pub exact: bool,

    /// Use regex instead of fuzzy match
    #[arg(long)]
    pub regex: bool,

    /// Fuzzy matching pub algorithm
    #[arg(value_enum, long = "algo", default_value_t = FuzzyAlgorithm::SkimV2)]
    pub algorithm: FuzzyAlgorithm,

    /// Case sensitive or not
    #[arg(value_enum, long, default_value_t = CaseMatching::Smart)]
    pub case: CaseMatching,

    /* Interface */
    /// Comma seperated keybindings, in pub KEY:ACTION,
    /// such as 'ctrl-pub j:accept,ctrl-k:kill-line'
    #[arg(short, long)]
    pub bind: Vec<String>,

    /// Enable Multiple Selection
    #[arg(short, long)]
    pub multi: bool,

    /// Disable Multiple Selection
    #[arg(long)]
    pub no_multi: bool,

    /// Disable mouse events
    #[arg(long)]
    pub no_mouse: bool,

    /// Command to invoke dynamically
    #[arg(short, long)]
    pub cmd: Option<String>,

    /// Start skim in interactive (command) mode
    #[arg(short, long)]
    pub interactive: bool,

    /// Change color theme
    #[arg(long)]
    pub color: Vec<String>,

    /// Disable horizontal scroll
    #[arg(long)]
    pub no_hscroll: bool,

    /// Keep the right end of the line visible on overflow
    #[arg(long)]
    pub keep_right: bool,

    /// Line starts with the start of matched pattern
    #[arg(long, default_value = "")]
    pub skip_to_pattern: String,

    /// Do not clear previous items if command returns empty result
    #[arg(long)]
    pub no_clear_if_empty: bool,

    /// Do not clear on start
    #[arg(long)]
    pub no_clear_start: bool,

    /// Send command error message if command fails
    #[arg(long)]
    pub show_cmd_error: bool,

    /* Layout */
    /// Choose layout
    #[arg(value_enum, long, default_value_t = Layout::Default)]
    pub layout: Layout,

    /// Shortcut for `layout='reverse'`
    #[arg(long)]
    pub reverse: bool,

    /// Height of skim's window (--height 40%)
    #[arg(long, default_value = "100%")]
    pub height: String,

    /// Disable height feature
    #[arg(long)]
    pub no_height: bool,

    /// Minimum height when --height is given by percent
    #[arg(long, default_value = "10")]
    pub min_height: String,

    /// Screen Margin (TRBL / TB,RL / T,RL,B / T,R,B,L)
    /// e.g. `sk --margin 1,10%`
    #[arg(long, default_values_t = vec!["0".to_owned(); 4])]
    pub margin: Vec<String>,

    /// Prompt string for query mode
    #[arg(short, long, default_value = "> ")]
    pub prompt: String,

    /// Prompt string for command mode
    #[arg(long, default_value = "c> ")]
    pub cmd_prompt: String,

    /* Display */
    /// Parse ANSI color codes for input strings
    #[arg(long)]
    pub ansi: bool,

    /// Number of spaces for a tab character
    #[arg(long, default_value_t = 8)]
    pub tabstop: usize,

    /// Display STR next to info
    #[arg(long)]
    pub header: Option<String>,

    /// The first N lines of the input are treated as header
    #[arg(long, default_value_t = 0)]
    pub header_lines: usize,

    /// Display info next to query
    #[arg(long)]
    pub inline_info: bool,

    /* History */
    /// History file
    #[arg(long)]
    pub history: Option<PathBuf>,

    /// Maximum number of query history entries
    #[arg(long, default_value_t = 1000)]
    pub history_size: usize,

    /// Command History file
    #[arg(long)]
    pub cmd_history: Option<PathBuf>,

    /// Maximum number of command history entries
    #[arg(long, default_value_t = 1000)]
    pub cmd_history_size: usize,

    /* Preview */
    /// Command to preview current highlighted line ({})
    /// We can specify the fields. e.g. ({1}, {..3}, {0..})
    #[arg(long)]
    pub preview: Option<String>,

    /// Preview window layout
    /// [up|down|left|right][:SIZE[%]][:hidden][:+SCROLL[-OFFSET]]
    #[arg(long, default_value = "right:50%")]
    pub preview_window: String,

    /* Scripting */
    /// Specify the initial query
    #[arg(short, long)]
    pub query: Option<String>,

    /// Specify the initial query for interactive mode
    #[arg(long)]
    pub cmd_query: Option<String>,

    /// Comma seperated keys that can be used to complete skim
    #[arg(long)]
    pub expect: Vec<String>,

    /// Read input delimited by ASCII NUL(\0) characters
    #[arg(long)]
    pub read0: bool,

    /// Print output delimited by ASCII NUL(\0) characters
    #[arg(long)]
    pub print0: bool,

    /// Do not clear screen on exit
    #[arg(long)]
    pub no_clear: bool,

    /// Filter mode,
    /// output the score and the item to stdout
    #[arg(short, long)]
    pub filter: Option<String>,

    /// Print query as the first line
    #[arg(long)]
    pub print_query: bool,

    /// Print command query as the first line (after --print-query)
    #[arg(long)]
    pub print_cmd: bool,

    /// Print matching score in filter output (with --filter)
    #[arg(long, requires = "filter")]
    pub print_score: bool,

    /// Automatically select the only match
    #[arg(short = '1', long)]
    pub select_1: bool,

    /// Exit immediately when there's no match
    #[arg(short = '0', long)]
    pub exit_0: bool,

    /// Synchronous search for multi-staged filtering
    #[arg(long)]
    pub sync: bool,

    /// Pre-select the first n items in multi-selection mode
    #[arg(long, default_value_t = 0)]
    pub pre_select_n: usize,

    /// Pre-select the matched items in multi-selection mode
    #[arg(long, default_value = "")]
    pub pre_select_pat: String,

    /// Pre-select the matched items in multi-selection mode
    #[arg(long)]
    pub pre_select_items: Option<String>,

    /// Pre-select the items read from file
    #[arg(long)]
    pub pre_select_file: Option<PathBuf>,

    /* Removed */
    /// Replace `replstr` with the selected item
    #[arg(short = 'I')]
    pub replstr: Option<String>,

    /* Reserved (not used for now) */
    #[arg(short = 'x', long)]
    pub extended: bool,

    #[arg(long)]
    pub literal: bool,

    #[arg(long)]
    pub cycle: bool,

    #[arg(long, default_value_t = 10)]
    pub hscroll_off: usize,

    #[arg(long)]
    pub filepath_word: bool,

    #[arg(long, default_value = "abcdefghijklmnopqrstuvwxyz")]
    pub jump_labels: String,

    #[arg(long)]
    pub border: bool,

    #[arg(long)]
    pub no_bold: bool,
}
