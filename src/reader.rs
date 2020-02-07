///! Reader is used for reading items from datasource (e.g. stdin or command output)
///!
///! After reading in a line, reader will save an item into the pool(items)
use crate::item::ItemWrapper;
use crate::item_collector::{read_and_collect_from_command, CollectorInput, CollectorOption};
use crate::options::SkimOptions;
use crate::spinlock::SpinLock;
use crate::SkimItemReceiver;
use crossbeam::channel::{bounded, select, Sender};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::thread;

const CHANNEL_SIZE: usize = 1024;

pub struct ReaderControl {
    tx_interrupt: Sender<i32>,
    tx_interrupt_cmd: Option<Sender<i32>>,
    components_to_stop: Arc<AtomicUsize>,
    items: Arc<SpinLock<Vec<Arc<ItemWrapper>>>>,
}

impl ReaderControl {
    pub fn kill(self) {
        debug!(
            "kill reader, components before: {}",
            self.components_to_stop.load(Ordering::SeqCst)
        );

        let _ = self.tx_interrupt_cmd.map(|tx| tx.send(1));
        let _ = self.tx_interrupt.send(1);
        while self.components_to_stop.load(Ordering::SeqCst) != 0 {}
    }

    pub fn take(&self) -> Vec<Arc<ItemWrapper>> {
        let mut items = self.items.lock();
        let mut ret = Vec::with_capacity(items.len());
        ret.append(&mut items);
        ret
    }

    pub fn is_done(&self) -> bool {
        let items = self.items.lock();
        self.components_to_stop.load(Ordering::SeqCst) == 0 && items.is_empty()
    }
}

pub struct Reader {
    option: CollectorOption,
    rx_item: Option<SkimItemReceiver>,
}

impl Reader {
    pub fn with_options(options: &SkimOptions) -> Self {
        Self {
            option: CollectorOption::with_options(&options),
            rx_item: None,
        }
    }

    pub fn source(mut self, rx_item: Option<SkimItemReceiver>) -> Self {
        self.rx_item = rx_item;
        self
    }

    pub fn run(&mut self, cmd: &str) -> ReaderControl {
        let components_to_stop: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
        let items = Arc::new(SpinLock::new(Vec::new()));
        let items_clone = items.clone();
        let option_clone = self.option.clone();
        let cmd = cmd.to_string();

        let run_num = if self.rx_item.is_some() {
            RUN_NUM.fetch_add(1, Ordering::SeqCst)
        } else {
            *NUM_MAP
                .write()
                .expect("reader: failed to lock NUM_MAP")
                .entry(cmd.to_string())
                .or_insert_with(|| RUN_NUM.fetch_add(1, Ordering::SeqCst))
        };

        let (rx_item, tx_interrupt_cmd) = self.rx_item.take().map(|rx| (rx, None)).unwrap_or_else(|| {
            let components_to_stop_clone = components_to_stop.clone();
            let (rx_item, tx_interrupt_cmd) =
                read_and_collect_from_command(components_to_stop_clone, CollectorInput::Command(cmd), option_clone);
            (rx_item, Some(tx_interrupt_cmd))
        });

        let components_to_stop_clone = components_to_stop.clone();
        let tx_interrupt = collect_item(components_to_stop_clone, rx_item, run_num, items_clone);

        ReaderControl {
            tx_interrupt,
            tx_interrupt_cmd,
            components_to_stop,
            items,
        }
    }
}

// Consider that you invoke a command with different arguments several times
// If you select some items each time, how will skim remember it?
// => Well, we'll give each invocation a number, i.e. RUN_NUM
// What if you invoke the same command and same arguments twice?
// => We use NUM_MAP to specify the same run number.
lazy_static! {
    static ref RUN_NUM: AtomicUsize = AtomicUsize::new(0);
    static ref NUM_MAP: RwLock<HashMap<String, usize>> = RwLock::new(HashMap::new());
}

fn collect_item(
    components_to_stop: Arc<AtomicUsize>,
    rx_item: SkimItemReceiver,
    run_num: usize,
    items: Arc<SpinLock<Vec<Arc<ItemWrapper>>>>,
) -> Sender<i32> {
    let (tx_interrupt, rx_interrupt) = bounded(CHANNEL_SIZE);

    let started = Arc::new(AtomicBool::new(false));
    let started_clone = started.clone();
    thread::spawn(move || {
        debug!("reader: collect_item start");
        components_to_stop.fetch_add(1, Ordering::SeqCst);
        started_clone.store(true, Ordering::SeqCst); // notify parent that it is started

        let mut index = 0;
        loop {
            select! {
                recv(rx_item) -> new_item => match new_item {
                    Ok(item) => {
                        let item_wrapped = ItemWrapper::new(item, (run_num, index));
                        let mut vec = items.lock();
                        vec.push(Arc::new(item_wrapped));
                        index += 1;
                    }
                    Err(_) => break,
                },
                recv(rx_interrupt) -> _msg => break,
            }
        }

        components_to_stop.fetch_sub(1, Ordering::SeqCst);
        debug!("reader: collect_item stop");
    });

    while !started.load(Ordering::SeqCst) {
        // busy waiting for the thread to start. (components_to_stop is added)
    }

    tx_interrupt
}
