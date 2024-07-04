# Source: https://github.com/tox-dev/sphinx-argparse-cli/blob/main/roots/test-complex/parser.py

from argparse import ArgumentParser


def parser() -> ArgumentParser:
    parser = ArgumentParser(description="argparse tester", prog="complex", epilog="test epilog")
    parser.add_argument("--root", action="store_true", help="root flag")
    parser.add_argument("--no-help", action="store_true")
    parser.add_argument("--outdir", "-o", type=str, help="output directory", metavar="out_dir")
    parser.add_argument("--in-dir", "-i", type=str, help="input directory", dest="in_dir")

    group = parser.add_argument_group("Exclusive", description="this is an exclusive group")
    exclusive = group.add_mutually_exclusive_group()
    exclusive.add_argument("--foo", action="store_true", help="foo")
    exclusive.add_argument("--bar", action="store_true", help="bar")

    parser.add_argument_group("empty")

    sub_parsers_a = parser.add_subparsers(title="sub-parser-a", description="sub parsers A", dest="command")
    sub_parsers_a.required = False
    sub_parsers_a.default = "first"

    a_parser_first = sub_parsers_a.add_parser("first", aliases=["f"], help="a-first-help", description="a-first-desc")
    a_parser_first.add_argument("--flag", dest="a_par_first_flag", action="store_true", help="a parser first flag")
    a_parser_first.add_argument("--root", action="store_true", help="root flag")
    a_parser_first.add_argument("pos_one", help="first positional argument", metavar="one")
    a_parser_first.add_argument("pos_two", help="second positional argument", default=1)

    a_parser_second = sub_parsers_a.add_parser("second")
    a_parser_second.add_argument("--flag", dest="a_par_second_flag", action="store_true", help="a parser second flag")
    a_parser_second.add_argument("--root", action="store_true", help="root flag")
    a_parser_second.add_argument("pos_one", help="first positional argument", metavar="one")
    a_parser_second.add_argument("pos_two", help="second positional argument", default="green")

    sub_parsers_a.add_parser("third")  # empty sub-command
    return parser


parser().parse_args()
