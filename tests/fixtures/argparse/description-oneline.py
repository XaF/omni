# Source: https://github.com/tox-dev/sphinx-argparse-cli/blob/main/roots/test-description-set/parser.py

from argparse import ArgumentParser


def parser() -> ArgumentParser:
    return ArgumentParser(prog="foo", description="desc")


parser().parse_args()
