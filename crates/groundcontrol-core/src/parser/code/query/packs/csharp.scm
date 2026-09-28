;; C# tags and definition query pack

(method_declaration
  name: (identifier) @name) @definition.method

(constructor_declaration
  name: (identifier) @name) @definition.method

(class_declaration
  name: (identifier) @name
  (base_list (identifier) @inherits)?) @definition.class

(interface_declaration
  name: (identifier) @name
  (base_list (identifier) @inherits)?) @definition.interface

(struct_declaration
  name: (identifier) @name) @definition.struct

(enum_declaration
  name: (identifier) @name) @definition.enum

(record_declaration
  name: (identifier) @name) @definition.class

(namespace_declaration
  name: (_) @name) @definition.module
