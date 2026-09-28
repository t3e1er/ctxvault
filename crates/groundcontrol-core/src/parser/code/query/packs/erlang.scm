;; Erlang tags and definition query pack

(module_attribute
  name: (atom) @name) @definition.module

(fun_decl
  clause: (function_clause
    name: (atom) @name)) @definition.function

(record_decl
  name: (atom) @name) @definition.struct
