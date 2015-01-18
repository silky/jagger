#![feature(box_syntax)]
#![allow(unstable)]
#[macro_use] extern crate log;
extern crate test;

mod metadata;
mod pkg;
mod solver;
mod dimacs;
mod formulator;

#[cfg(not(test))]
fn main() {

}