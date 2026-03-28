#!/bin/bash

source ./utils.sh
. ./helpers.sh

# Greet a person by name.
function greet() {
    echo "Hello, $1"
}

say_hello() {
    echo "Hi there"
}

goodbye() {
    echo "Bye"
}
