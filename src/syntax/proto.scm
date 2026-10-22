(identifier) @variable
(comment) @comment
(string) @string
["\"proto2\"" "\"proto3\""] @string
(escape_sequence) @string.escape
[(int_lit) (float_lit)] @number
(bool) @boolean
[(message_name) (enum_name) (message_or_enum_type) (type) (key_type)] @type
[(service_name) (rpc_name)] @function
(field (identifier) @property)
[
  "syntax" "edition" "package" "import" "option" "message" "enum" "service"
  "rpc" "returns" "repeated" "optional" "required" "oneof" "map" "reserved"
  "extensions" "extend" "stream" "public" "weak" "to" "max"
] @keyword
["{" "}" "(" ")" "[" "]" "<" ">"] @punctuation.bracket
[";" "," "=" "."] @punctuation.delimiter
