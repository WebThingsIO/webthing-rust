#!/bin/bash -e

# Manually build OpenSSL. The openssl create requires 1.0.2+, but Travis CI
# only includes 1.0.0.
wget https://www.openssl.org/source/openssl-1.1.0h.tar.gz
tar xzf openssl-1.1.0h.tar.gz
cd openssl-1.1.0h
./config --prefix=/usr/local
make >/dev/null
sudo make install >/dev/null
sudo ldconfig
cd ..

# build library
cargo build

# clone the webthing-tester
git clone https://github.com/mozilla-iot/webthing-tester
pip3 install --user -r webthing-tester/requirements.txt

# build and test the single-thing example
cargo run --example single-thing &
EXAMPLE_PID=$!
sleep 5
./webthing-tester/test-client.py
kill -15 $EXAMPLE_PID

# build and test the multiple-things example
cargo run --example multiple-things &
EXAMPLE_PID=$!
sleep 5
./webthing-tester/test-client.py --path-prefix "/0"
kill -15 $EXAMPLE_PID
