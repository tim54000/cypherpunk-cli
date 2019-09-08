# Cypherpunk CLI

## Table of Contents
+ [About](#about)
+ [Getting Started](#getting_started)
+ [Usage](#usage)
+ [Contributing](../CONTRIBUTING.md)

## About
Cypherpunk CLI is a tool that simplifies your life when you want to send 
an email between several remailers. It was created to have an alternative 
to MixMaster (latest version from 2008) to simply send messages without having
to break your head to install and use it and not have to use GPG manually.

All messages will be encrypted and distributed as PGP messages.

## Getting Started
These instructions will get you a copy of the project up and running on your local machine.


### Prerequisites

First of all, you need Cap'n'proco if you build the tool with sequoiaPGP backend, 
otherwise you don't actually need it!
And of course, Rust =)

#### Cap'n'proto

> Only for SequoiaPGP backend \[actually don't work]

For Debian/Ubuntu: run ```$ apt-get install capnproto```  
For OSX: run ```$ brew install capnp```  
For Arch distribs: run ```$ pacman -Sy capnproto```

For others systems, follow the official guide at https://capnproto.org/install.html

#### Rust

To build source, you need to have a correct Rust toolchain and compilator.
If you don't have yet one, check https://rustup.rs/

#### Build it!

Clone the git repository, with `git clone` and build the tool using:  
```SHELL
$ cargo build --package cypherpunk-cli --release
```

Enjoy, you built it!   
The final executable is in the folder `./target/release`

On Unix systems, run `./target/release/cypherpunk-cli --help`  
On Windows, run `./target/release/cypherpunk-cli.exe --help`

### Installing

The installation has not yet been thought of, for the moment just use the binary you created

## Usage

The use of the tool is still unstable and can change at any time. Use the `--help`
option for now.

#### Message format:
Remailer-valid formatted message seems to: 
```

::
Anon-To: <email_recipient>
Header_for_Remailer: Value visible only by the remailer

##
Subject: A subject
Another_Header_Present_In_The_Final_Message: A value for it

Message's body here!
```
More info: http://www.panta-rhei.dyndns.org/JBNR-en.htm#CForm

#### Tool usage:
* Encrypt message named `./message.txt`, chain: two random remailers:
```
$ ./target/release/cypherpunk-cli ./message.txt --chian "*" "*"
```

* Encrypt message named `./message.txt`, chain to paranoia -> dizum:
```
$ ./target/release/cypherpunk-cli ./message.txt --chain paranoia dizum
```

* Encrypt message named `./message.txt`, chain: two random remailers, redundancy: 2 messages:
```
$ ./target/release/cypherpunk-cli ./message.txt --chain "*" "*" -r 2
```

* Encrypt message named `./message.txt`, chain: two random remailers, formatted to mailto URL:
```
$ ./target/release/cypherpunk-cli ./message.txt --chain "*" "*" --mailto
```