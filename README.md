# Android File Transfer Service

A small service/script to transfer files from an android device to a location on a hd. When the service is configured and running, automatically detects a designated connected device, and starts copying files from a designated location on the device, to another designated location on host disk. Note that when the usb is plugged in, the connection must still be auhtorized on the device UI. Logs transferred files so that files will get only copied over once. If a file is then manually removed from the destination directory, on next plugin of the device the file is not copied over again.

Developed and tested on Ubuntu. Uses gvfs-copy to copy files using MTP. Use whatever means you can to make MTP file transfers work on your machine, as I had to resort to a few different methods to get it working in the first place.

This project was made 50% to fill a need and 50% to learn Rust.
