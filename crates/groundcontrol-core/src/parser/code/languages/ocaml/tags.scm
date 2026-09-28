(value_definition
  (let_binding
    pattern: (value_name) @name)) @definition.function

(type_definition
  (type_binding
    name: (type_constructor) @name)) @definition.type

(module_definition
  (module_binding
    (module_name) @name)) @definition.module