extern crate libc;

use std::io::{stdin, Read, BufRead, BufReader};
use std::error::Error;
use std::sync::{Arc, Mutex};
use std::thread;
use std::process::{Command, Stdio, exit};

// invoke find comand.
fn get_command_output() -> Result<Box<BufRead>, Box<Error>> {
    let command = try!(Command::new("find")
                       .arg(".")
                       .stdout(Stdio::piped())
                       .stderr(Stdio::null())
                       .spawn());
    let stdout = try!(command.stdout.ok_or("command output: unwrap failed".to_owned()));
    Ok(Box::new(BufReader::new(stdout)))
}

fn reader(mtx: Arc<Mutex<Vec<String>>>) {
    // check if the input is TTY
    let istty = unsafe { libc::isatty(libc::STDIN_FILENO as i32) } != 0;

    let mut read;
    if istty {
        read = get_command_output().expect("command not found: find");
    } else {
        read = Box::new(BufReader::new(stdin()))
    };

    loop {
        let mut input = String::new();
        match read.read_line(&mut input) {
            Ok(n) => {
                if n <= 0 { break; }

                if input.ends_with("\n") {
                    input.pop();
                    if input.ends_with("\r") {
                        input.pop();
                    }
                }

                let mut items = mtx.lock().unwrap();
                (*items).push(input);
            }

            Err(_err) => { break; }
        }
    }
}


fn main() {
    let mtx_item = Arc::new(Mutex::new(vec![]));

    // reader
    let reader_mtx_item = mtx_item.clone();
    let reader = thread::spawn(|| reader(reader_mtx_item));

    // displayer
    let displayer_mtx_item = mtx_item.clone();
    let displayer = thread::spawn(move || {
        loop {
            let mut items = displayer_mtx_item.lock().unwrap();
            if (*items).len() > 0 {
                println!("Got: {:?}", *items);
                *items = vec![];
            }
        }
    });

    reader.join();
    displayer.join();
}
