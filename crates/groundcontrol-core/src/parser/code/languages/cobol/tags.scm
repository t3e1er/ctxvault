(program_definition
  (identification_division
    (program_name) @name)) @definition.module

(paragraph_header
  name: (WORD) @name) @definition.function

(section_header
  name: (WORD) @name) @definition.function

(call_statement
  x: [(string) (WORD)] @name) @reference.call

(copy_statement
  book: [(string) (WORD)] @name) @reference.import
