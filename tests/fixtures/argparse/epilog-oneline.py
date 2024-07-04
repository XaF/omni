# Source: https://github.com/tox-dev/sphinx-argparse-cli/blob/main/roots/test-epilog-set/parser.py

from argparse import ArgumentParser


def parser() -> ArgumentParser:
    return ArgumentParser(prog="foo", epilog="epi")


parser().parse_args()
