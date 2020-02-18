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

First of all, you need Rust =)

#### Rust

To build source, you need to have a correct Rust toolchain and compiler.
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

The installation hasn't been thought about yet, for now, just use the binary you created. 
However if you really want to add Cypherpunk in your path, you can try:

```SHELL
$ cargo install --package cypherpunk-cli --release
```

However, you may need to add the `remailers.json` config to the same directory otherwise 
it won't work.

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
(if down: [QmboFHizh9ys57DXcVsniVDYS46gsiBP716u2sqQE7xgV4](https://gateway.ipfs.io/ipfs/QmboFHizh9ys57DXcVsniVDYS46gsiBP716u2sqQE7xgV4))

#### Tool usage:
* Encrypt message from stdin, chain with two random remailer:
```
$ cypherpunk-cli --chain "*" "*"
```

* Encrypt message named `./message.txt`, chain with paranoia and dizum:
```
$ cypherpunk-cli --input ./message.txt --chain paranoia dizum
```

* Encrypt message named `./message.txt`, chain with paranoia and dizum, saved into `./out/`:
```
$ cypherpunk-cli --input ./message.txt --chain paranoia dizum --output ./out/
```

* Encrypt message named `./message.txt`, chain with paranoia and dizum, redundancy: 2 messages:
```
$ cypherpunk-cli --input ./message.txt --chain paranoia dizum --redundancy 2
```

* Encrypt message named `./message.txt`, chain with two random remailer, formatted to mailto URL:
```
$ cypherpunk-cli --input ./message.txt --chain "*" "*" --format mailto
```

* Encrypt message named `./message.txt`, chain with austria, formatted to EML file:
```
$ cypherpunk-cli --input ./message.txt --chain austria --format eml
```

##### cypherpunk --help
```SHELL
cypherpunk x.x.x
CLI tool to encrypt your messages between different remailers easily

USAGE:
    cypherpunk-cli [FLAGS] [OPTIONS]

FLAGS:
    -h, --help       Prints help information
    -q, --quiet      The quiet flag to make the PGP backend quiet and soon more...
    -V, --version    Prints version information

OPTIONS:
    -c, --chain <chain>...           The remailer chain through which your message will pass. [required] Tips: you can
                                     use a joker "*" to randomly choose one remailer in the config. It will change with
                                     each redundant message
    -f, --format <format>            The output message format, by default it will be in Cypherpunk format [default:
                                     cypherpunk]  [possible values: Cypherpunk, Mailto, EML]
    -i, --input <input>              Messsage input file, stdin if not present; the message must be readable by the last
                                     Cypherpunk remailer in the chain
    -o, --output <output>            Output dir, stdout if not present; all the encrypted message for remailer will be
                                     there
    -r, --redundancy <redundancy>    Number of redundancy message to encrypted because Cypherpunk may forgot your
                                     message. If you use a "*" for remailer it will be randomly choose for each
                                     redundancy message [default: 1]

```