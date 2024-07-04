# Source: https://github.com/tox-dev/sphinx-argparse-cli/blob/main/roots/test-basic/parser.py

from argparse import ArgumentParser


def parser() -> ArgumentParser:
    return ArgumentParser(prog="basic")


parser().parse_args()
