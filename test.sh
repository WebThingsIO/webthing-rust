#!/bin/bash -e

# build library
cargo build
cargo build --features ssl

# clone the webthing-tester
if [ ! -d webthing-tester ]; then
    git clone https://github.com/mozilla-iot/webthing-tester
fi
pip3 install --user -r webthing-tester/requirements.txt

# build and test the single-thing example
cargo build --example single-thing
cargo run --example single-thing &
EXAMPLE_PID=$!
sleep 5
./webthing-tester/test-client.py
kill -15 $EXAMPLE_PID

# build and test the multiple-things example
cargo build --example multiple-things
cargo run --example multiple-things &
EXAMPLE_PID=$!
sleep 5
./webthing-tester/test-client.py --path-prefix "/0"
kill -15 $EXAMPLE_PID
