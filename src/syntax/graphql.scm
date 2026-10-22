(name) @variable
(comment) @comment
(description) @string
(string_value) @string
[(int_value) (float_value)] @number
(boolean_value) @boolean
(null_value) @constant
(named_type) @type
(fragment_name) @function
(variable) @variable
(field (name) @property)
(directive (name) @attribute)
[
  "query" "mutation" "subscription" "fragment" "on" "schema" "type"
  "input" "interface" "union" "scalar" "enum" "extend" "implements"
  "directive" "repeatable"
] @keyword
["{" "}" "(" ")" "[" "]"] @punctuation.bracket
[":" "!" "=" "@" "$" "|"] @punctuation.delimiter
