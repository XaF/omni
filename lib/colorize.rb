require 'colorize'

# If we don't have a tty, we want to disable colorization
String.disable_colorization = true unless STDERR.tty?
