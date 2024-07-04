# Source: https://github.com/tox-dev/sphinx-argparse-cli/blob/main/roots/test-description-multiline/parser.py

from argparse import ArgumentParser, RawDescriptionHelpFormatter


def parser() -> ArgumentParser:
    parser = ArgumentParser(
        prog="foo",
        description="""This description
spans multiple lines.

  this line is indented.
    and also this.

Now this should be a separate paragraph.
""",
        formatter_class=RawDescriptionHelpFormatter,
    )
    group = parser.add_argument_group(
        description="""This group description

spans multiple lines.
"""
    )
    group.add_argument("--dummy")
    return parser


parser().parse_args()
