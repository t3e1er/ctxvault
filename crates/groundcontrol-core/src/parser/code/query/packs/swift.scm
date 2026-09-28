;; Swift tags and definition query pack

(function_declaration
  name: (simple_identifier) @name) @definition.function

(class_declaration
  name: (type_identifier) @name
  (inheritance_specifier
    inherits_from: (user_type
      (type_identifier) @inherits))?) @definition.class

(protocol_declaration
  name: (type_identifier) @name
  (inheritance_specifier
    inherits_from: (user_type
      (type_identifier) @inherits))?) @definition.interface
