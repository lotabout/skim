use crate::global::mark_new_run;
///! Reader is used for reading items from datasource (e.g. stdin or command output)
///!
///! After reading in a line, reader will save an item into the pool(items)
use crate::item_collector::{read_and_collect_from_command, CollectorInput, CollectorOption};
use crate::options::SkimOptions;
use crate::spinlock::SpinLock;
use crate::{SkimItem, SkimItemReceiver};
use crossbeam::channel::{bounded, select, Sender};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

const CHANNEL_SIZE: usize = 1024;

pub struct ReaderControl {
    tx_interrupt: Sender<i32>,
    tx_interrupt_cmd: Option<Sender<i32>>,
    components_to_stop: Arc<AtomicUsize>,
    items: Arc<SpinLock<Vec<Arc<dyn SkimItem>>>>,
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

    pub fn take(&self) -> Vec<Arc<dyn SkimItem>> {
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
        mark_new_run(cmd);

        let components_to_stop: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
        let items = Arc::new(SpinLock::new(Vec::new()));
        let items_clone = items.clone();
        let option_clone = self.option.clone();
        let cmd = cmd.to_string();

        let (rx_item, tx_interrupt_cmd) = self.rx_item.take().map(|rx| (rx, None)).unwrap_or_else(|| {
            let components_to_stop_clone = components_to_stop.clone();
            let (rx_item, tx_interrupt_cmd) =
                read_and_collect_from_command(components_to_stop_clone, CollectorInput::Command(cmd), option_clone);
            (rx_item, Some(tx_interrupt_cmd))
        });

        let components_to_stop_clone = components_to_stop.clone();
        let tx_interrupt = collect_item(components_to_stop_clone, rx_item, items_clone);

        ReaderControl {
            tx_interrupt,
            tx_interrupt_cmd,
            components_to_stop,
            items,
        }
    }
}

fn collect_item(
    components_to_stop: Arc<AtomicUsize>,
    rx_item: SkimItemReceiver,
    items: Arc<SpinLock<Vec<Arc<dyn SkimItem>>>>,
) -> Sender<i32> {
    let (tx_interrupt, rx_interrupt) = bounded(CHANNEL_SIZE);

    let started = Arc::new(AtomicBool::new(false));
    let started_clone = started.clone();
    thread::spawn(move || {
        debug!("reader: collect_item start");
        components_to_stop.fetch_add(1, Ordering::SeqCst);
        started_clone.store(true, Ordering::SeqCst); // notify parent that it is started

        loop {
            select! {
                recv(rx_item) -> new_item => match new_item {
                    Ok(item) => {
                        let mut vec = items.lock();
                        vec.push(item);
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
