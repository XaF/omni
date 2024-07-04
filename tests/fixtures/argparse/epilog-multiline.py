# Source: https://github.com/tox-dev/sphinx-argparse-cli/blob/main/roots/test-epilog-multiline/parser.py

from argparse import ArgumentParser, RawDescriptionHelpFormatter


def parser() -> ArgumentParser:
    return ArgumentParser(
        prog="foo",
        epilog="""This epilog
spans multiple lines.

  this line is indented.
    and also this.

Now this should be a separate paragraph.
""",
        formatter_class=RawDescriptionHelpFormatter,
    )


parser().parse_args()
