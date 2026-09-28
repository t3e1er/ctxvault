;; Java tags and definition query pack

(method_declaration
  name: (identifier) @name) @definition.method

(constructor_declaration
  name: (identifier) @name) @definition.method

(class_declaration
  name: (identifier) @name
  superclass: (superclass (type_identifier) @inherits)?
  interfaces: (super_interfaces (type_list (type_identifier) @implements))?) @definition.class

(interface_declaration
  name: (identifier) @name
  (extends_interfaces (type_list (type_identifier) @inherits))?) @definition.interface

(enum_declaration
  name: (identifier) @name) @definition.enum

(record_declaration
  name: (identifier) @name) @definition.class

;; Spring Boot: @GetMapping("/path"), @PostMapping("/path"), @RequestMapping("/path")
(method_declaration
  (modifiers
    (annotation
      name: (identifier) @_ann (#match? @_ann "^(GetMapping|PostMapping|PutMapping|DeleteMapping|PatchMapping|RequestMapping)$")
      arguments: (annotation_argument_list
        (string_literal) @name)))
  name: (identifier) @handles) @definition.route
