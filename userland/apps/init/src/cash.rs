use core::io::{self, ErrorKind};

use alloc::{
    format,
    io::Read,
    string::{String, ToString},
    vec::Vec,
};
use runtime::{self as rt, print, println};

pub fn run() -> isize {
    loop {
        print!("# ");

        let input_str = match read_line_with_echo() {
            Ok(s) => s,
            Err(e) => {
                println!("Error reading input: {e}");
                continue;
            }
        };

        println!(); // Move to the next line after input

        match eval(&input_str) {
            Ok(Some(exit_code)) => return exit_code,
            Ok(None) => continue,
            Err(e) => {
                println!("cash: {e}");
                continue;
            }
        }
    }
}

fn read_line_with_echo() -> Result<String, io::Error> {
    let mut input = Vec::new();
    loop {
        let mut buf = [0u8; 1];
        match rt::io::stdin().read(&mut buf) {
            Ok(0) => break,
            Ok(_) => match buf[0] {
                b'\n' => break,
                b'\x7f' | b'\x08' => {
                    if input.pop().is_some() {
                        print!("\x08 \x08");
                    }
                }
                c => {
                    input.push(c);
                    print!("{}", c as char);
                }
            },
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                continue;
            }
            Err(e) => return Err(e),
        }
    }

    match str::from_utf8(&input) {
        Ok(s) => Ok(s.trim().to_string()),
        Err(_) => Err(io::Error::new(
            ErrorKind::InvalidData,
            "Invalid UTF-8 input",
        )),
    }
}

fn eval(input: &str) -> Result<Option<isize>, String> {
    let mut tkz = Tokenizer::new(input);
    let mut tokens = tkz.iter();

    match tokens.next() {
        Some("exit") => {
            let exit_code = match tokens.next() {
                Some(code_str) => match code_str.parse::<isize>() {
                    Ok(code) => code,
                    Err(e) => return Err(format!("Invalid exit code: {e}")),
                },
                None => 0,
            };
            Ok(Some(exit_code))
        }
        Some("echo") => {
            let message: String = tokens.collect::<Vec<_>>().join(" ");
            println!("{message}");
            Ok(None)
        }
        Some(cmd) => Err(format!("Unknown command: {cmd}")),
        None => Ok(None),
    }
}

struct Tokenizer<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Tokenizer<'a> {
    pub fn new(input: &'a str) -> Self {
        Tokenizer { input, pos: 0 }
    }

    pub fn iter(&'a mut self) -> Tokens<'a> {
        Tokens { tokenizer: self }
    }

    fn next_token(&mut self) -> Option<&'a str> {
        self.skip_whitespace();
        if self.pos >= self.input.len() {
            return None;
        }

        let start = self.pos;
        while self.pos < self.input.len()
            && !self.input[self.pos..].starts_with(char::is_whitespace)
        {
            self.pos += 1;
        }
        Some(&self.input[start..self.pos])
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.input.len() && self.input[self.pos..].starts_with(char::is_whitespace)
        {
            self.pos += 1;
        }
    }
}

struct Tokens<'a> {
    tokenizer: &'a mut Tokenizer<'a>,
}

impl<'a> Iterator for Tokens<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        self.tokenizer.next_token()
    }
}
