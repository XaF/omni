# Source https://github.com/tox-dev/sphinx-argparse-cli/blob/main/roots/test-lower-upper-refs/parser.py

from argparse import ArgumentParser


def parser() -> ArgumentParser:
    parser = ArgumentParser(prog="basic")
    parser.add_argument("-d")
    parser.add_argument("-D")
    return parser


parser().parse_args()
