from argparse import ArgumentParser


def parser() -> ArgumentParser:
    parser = ArgumentParser(prog="positional-arg")
    parser.add_argument("x", help="arg")
    return parser


parser().parse_args()
