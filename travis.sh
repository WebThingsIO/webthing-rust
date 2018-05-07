#!/bin/bash -e

# build library
cargo build

# clone the webthing-tester
git clone https://github.com/mozilla-iot/webthing-tester
pip3 install --user -r webthing-tester/requirements.txt

# build and test the single-thing example
cd example/single-thing
cargo build
cargo run &
EXAMPLE_PID=$!
cd ../../
./webthing-tester/test-client.py
kill -15 $EXAMPLE_PID

# build and test the multiple-things example
cd example/multiple-things
cargo build
cargo run &
EXAMPLE_PID=$!
cd ../../
./webthing-tester/test-client.py --path-prefix "/0"
kill -15 $EXAMPLE_PID
