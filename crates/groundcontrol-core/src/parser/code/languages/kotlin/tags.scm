(function_declaration
  name: (identifier) @name) @definition.function

(class_declaration
  name: (identifier) @name
  (delegation_specifiers
    (delegation_specifier
      (user_type
        (identifier) @inherits)))?) @definition.class

(object_declaration
  name: (identifier) @name
  (delegation_specifiers
    (delegation_specifier
      (user_type
        (identifier) @inherits)))?) @definition.class