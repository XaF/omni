# Source https://github.com/tox-dev/sphinx-argparse-cli/blob/main/roots/test-group-title-prefix-custom/parser.py

from argparse import ArgumentParser


def parser() -> ArgumentParser:
    parser = ArgumentParser(description="argparse tester", prog="prog")
    parser.add_argument("root")
    parser.add_argument("--root", action="store_true", help="root flag")

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
    return parser


parser().parse_args()
