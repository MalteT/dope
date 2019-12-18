![logo](static/logo.png)
# **dope**, the **do**tfile-**p**r**e**processor.

Preprocess and link your dotfiles using a simple configuration and an easy preprocessor syntax. See the [`example`](./example) directory for a working example and a commented configuration file. The example configuration files contain usage examples of preprocessor instructions.

## Preprocessing

The preprocessing is split into two main operations.

1. **Evaluating preprocessor instructions** and
2. **Inserting Substitutions**

## Evaluating preprocessor instructions

Preprocessor instructions can be used to create different variations of your configuration files for different machines, while keeping a united configuration. They can also be used to create comments in comment-agnostic languages like JSON. The have the following syntax:
```
    PREFIX COMMAND
```
The prefix can be defined in the `preprocessor.toml`. The following commands can be used:

#### `IF` *expr*

If *expr* evaluates to `truish` all lines until an `ELSE` or `ENDIF` are transfered
to the processed file. If *expr* evaluates to a `falsy` value, the following lines
are deleted. If this command is followed by an `ELSE`-line, the opposite is done
to the lines between the `ELSE` and the `ENDIF`. `IF`-lines must be followed by an
`ENDIF`-line or an error is thrown. I.e. with `prefix = "#~"`
```
#~ IF $TERM == alacritty
wayland = true
#~ ELSE
wayland = false
#~ ENDIF
```

#### `IFDEF` *var*

Like `IF` but *var* is considered `truish`, if *var* is defined. That is, *var* contains
more than just whitespaces. Thus the only thing, that will evaluate to `falsy` is an
`IFDEF` followed by any number of whitespaces. I.e. with `prefix = "#~"`
```
#~ IFDEF $BLUB
sea: true
#~ ELSE
sea: false
#~ ENDIF
```

#### `IFNDEF` *var*

The opposite of `IFDEF`, everything but whitespaces is considered `falsy`

#### `ASK` *question*

If you want to let the user select a part of the configuration file you can use the `ASK` instruction. The *question* will be shown to the user with the possible options he may choose from. The options are given by `OPTION`-lines. The selection is ended by an `ENDASK`-line. I.e. with `prefix = "#~"` given:
```
#~ ASK What's your favourite color?
#~ OPTION RED
DEFAULT_COLOR=#FF0000
#~ OPTION GREEN
DEFAULT_COLOR=#00FF00
#~ OPTION BLUE
DEFAULT_COLOR=#0000FF
#~ ENDASK
```
The above example would display a prompt like this:
```text
ASK : What's your favourite color?
    :   1) RED
    :   2) GREEN
    :   3) BLUE
    : Enter a number: [1-3] >
```
If no `OPTION`-line is present, the user will be prompted with the *quest* and can answer `yes` or `no`, deciding whether to include the lines between `ASK` and `ENDASK`. **Note**: If the same *question* with the same options appears more than once, the choosen option will be used for all subsequent occurences. This even works across configuration files.
```
#~ ASK Is this a laptop?
battery_percentage_display = true
#~ ENDASK

[...]

#~ ASK Is this a laptop?
cpu_frequency = "lowest"
#~ ENDASK
```

#### `#` *comment*

This can be used to comment the source configuration file. I.e.:
```json
{
  #~ # This is a strange thing
  "strange": "thing",
  #~# This is another thing
  "other": "thing"
}
```

### Syntax of *var* and *expr*

A *var* is any valid unicode string. Before evaluation of *var*, all enviroment variables are expanded. Environment variables may only contain the characters `a-z`, `A-Z` and `_`. Two forms are understood: `${ENV_VARIABLE}` and `$ENV_VARIABLE`. Commands are also expanded and need to specified like this: `$(SOME command --with options | and --stuff)` All closing parenthesis `)` need to be escaped with a backslash. The command is run and replaced by its standard output.

An *expr* is always of the form "*var_1* == *var_2*". Both sides are expanded as mentioned above and checked for string equality, that is: All characters have to be equal.

## Inserting substitutions

Substitutions are defined in the `preprocessor.toml` under the `[substitutions]` key, i.e.:
```toml
[substitutions]
key = "Value"
NAME = "Max Mustermann"
Answer = 42
GREEN = "#00ff00"
```
