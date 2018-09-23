#[macro_use]
extern crate log;
extern crate chrono;
extern crate log4rs;

use std::collections::BTreeSet;
use std::fs;
use std::io::{BufRead, BufReader, Result, Write};
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::Duration;

use chrono::Local;


// template for the final source directory location where to search files from.
// the __BUS__ and __DEVICE__ will be replaced in the string once finding the
// correct device using the ``lsusb`` command, and searching the output using
// DEVICE_NAME.
const SOURCE_DIR_TEMPLATE: &str = "/run/user/1000/gvfs/mtp:host=%5Busb%3A__BUS__%2C__DEVICE__%5D/Phone/DCIM/Camera";
const DESTINATION_DIR: &str = "/home/hannu/move/files/to/path";
const DEVICE_NAME: &str = "Samsung";
// file to log entries in for successful transfers. logs date and number of files transferred
const LOG_FILE: &str = "/home/hannu/move/files/to/path/successful_transfers.log";
// file to keep list of transferred file names
const TRANSFERRED_FILES_FILE: &str = "/home/hannu/move/files/to/path/transferred_files.txt";
const WAIT_TIME_CONNECT_LOOP: u64 = 5;
const WAIT_TIME_DISCONNECT_LOOP: u64 = 5;


fn path_exists(path: &str) -> bool {
    Path::new(path).exists()
}

fn load_transferred_files(transferred_files_file: &str) -> Result<BTreeSet<String>> {
    // Load previously transferred list of files, represented in a BTreeSet.
    // If the file does not exist, it is created.
    debug!("Loading transferred files...");
    let file = fs::OpenOptions::new()
        .write(true)
        .read(true)
        .create(true)
        .open(transferred_files_file)
        .unwrap();
    let mut file_list: BTreeSet<String> = BTreeSet::new();
    for line in BufReader::new(file).lines() {
        file_list.insert(line.unwrap());
    }
    Ok(file_list)
}

fn append_lines_to_file(file_path: &str, lines: &[&str]) -> Result<()> {
    // Append contents of lines to file in file_path. Create target file if it does not exist.
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .append(true)
        .open(file_path)
        .unwrap();
    for line in lines {
        if let Err(e) = writeln!(file, "{}", line) {
            eprintln!("Couldn't write to file: {}", e);
        }
    }
    Ok(())
}

fn save_transferred_files(transferred_files_file: &str, transferred_files: &BTreeSet<String>) -> Result<()> {
    // Append new transferred files into the existing file that keeps track of transferred files.
    debug!("Updating list of transferred files...");
    let mut lines: Vec<&str> = Vec::new();
    for file_path in transferred_files {
        lines.push(file_path);
    }
    append_lines_to_file(transferred_files_file, &lines)?;
    Ok(())
}

fn log_action(log_file: &str, transferred_files_count: &i32) -> Result<()> {
    debug!("Logging transfer action...");
    let count_as_string = format!(
        "{} transferred files: {}",
        Local::now().to_string(),
        transferred_files_count.to_string()
    );
    let mut lines: Vec<&str> = Vec::new();
    lines.push(&count_as_string);
    append_lines_to_file(log_file, &lines)?;
    Ok(())
}

fn transfer_files(source_dir: &str, destination_dir: &str) -> Result<()> {
    // Copy files from source_dir to destination_dir. Save destination file paths
    // of copied files to a file transferred_files.txt, in the directory destination_dir.
    // Files that have been once previously transferred, will not get transferred again.

    info!("Begin syncing files from source: {} to destination: {}...", source_dir, destination_dir);

    let previously_transferred_files = load_transferred_files(TRANSFERRED_FILES_FILE)?;
    let mut new_transferred_files: BTreeSet<String> = BTreeSet::new();
    let mut count_files_transferred = 0;

    if ! path_exists(destination_dir) {
        info!("Creating destination directory: {}...", destination_dir);
        fs::create_dir(destination_dir).unwrap();
    }

    info!("Beginning transfer...");

    for dir_entry_result in fs::read_dir(source_dir)? {
        let entry = dir_entry_result.unwrap();
        let destination_file_path = format!("{}/{}", destination_dir, entry.file_name().into_string().unwrap());

        if previously_transferred_files.contains(&destination_file_path) {
            // files that have already been transferred once, even if manually removed
            // from the destination directory, should never be transferred again.
            continue;
        }

        let source_file_path = entry.path().into_os_string().into_string().unwrap();

        // copy from MTP (Media Transfer Protocol) file system requires a bit
        // more special method to copy files from...
        Command::new("gvfs-copy")
            .arg(&source_file_path)
            .arg(&destination_file_path)
            .spawn()
            .expect("Failed to copy file");

        // the above command only seems to initiate the transfer, and then return immediately,
        // even if using .spawn().unwrap().wait()... so best to sleep manually between
        // each transfer start, to give each file transfer some time to proceed and not
        // clog down the entire machine with potentially hundreads of simultaenous transfers.
        // most files are just photos, and 1 second will give a good headstart.
        thread::sleep(Duration::from_millis(1000));

        new_transferred_files.insert(destination_file_path);
        count_files_transferred += 1;

        if count_files_transferred % 10 == 0 {
            info!("{} files transferred", count_files_transferred);
        }
    }

    if count_files_transferred > 0 {
        save_transferred_files(TRANSFERRED_FILES_FILE, &new_transferred_files)?;
        log_action(LOG_FILE, &count_files_transferred)?;
        info!("Transferred {} new files", count_files_transferred);
        info!("Transfer complete");
    }
    else {
        info!("No new files found; did nothing.");
    }

    Ok(())
}

fn device_is_connected(device_name: &str, source_dir_template: &str) -> Option<String> {
    // find device by device_name, return full directory path to that device.
    let output = Command::new("lsusb")
        .output()
        .expect("Failed to list usb devices");

    let output_string = String::from_utf8_lossy(&output.stdout);
    let output_lines: Vec<&str> = output_string.split("\n").collect();

    for line in output_lines {
        if line.contains(device_name) {
            // line looks something like:
            // Bus 003 Device 026: ID 04e8:6860 Samsung Electronics Co., Ltd Galaxy (MTP)
            debug!("Device connected: {}", line);
            let usb_bus = &line[4..7];
            let usb_device = &line[15..18];
            let mut source_dir = String::from(source_dir_template);
            source_dir = str::replace(&source_dir, "__BUS__", usb_bus);
            source_dir = str::replace(&source_dir, "__DEVICE__", usb_device);
            if path_exists(&source_dir) {
                debug!("MTP Connections OK: Source dir found");
                return Some(source_dir)
            }
            else {
                debug!("Device connected, but MTP connections not permitted on device");
                break;
            }
        }
    }
    None
}

fn main() {
    log4rs::init_file("log4rs.yml", Default::default()).unwrap();
    info!("Service started");
    info!("Waiting for device to connect...");
    loop {
        let source_dir = match device_is_connected(DEVICE_NAME, SOURCE_DIR_TEMPLATE) {
            None => {
                debug!("Waiting for device to connect...");
                thread::sleep(Duration::from_millis(WAIT_TIME_CONNECT_LOOP * 1000));
                continue;
            },
            Some(source_dir) => source_dir,
        };

        match transfer_files(&source_dir, DESTINATION_DIR) {
            Ok(_result) => info!("Re-connect device to begin a new transfer."),
            Err(error)  => error!("File transfer resulted in an error: {}. Re-connect device to try again.", error),
        }

        loop {
            if path_exists(&source_dir) {
                debug!("Waiting for device to disconnect...");
                thread::sleep(Duration::from_millis(WAIT_TIME_DISCONNECT_LOOP * 1000));
            }
            else {
                info!("Device disconnected");
                break;
            }
        }
    }
}
