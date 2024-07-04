from argparse import ArgumentParser


def parser() -> ArgumentParser:
    parser = ArgumentParser(description="argparse tester", prog="positional-argument")
    parser.add_argument("meerkat", help="meerkat argument")
    parser.add_argument("--meerkat", action="store_true", help="meerkat flag")
    return parser


parser().parse_args()
