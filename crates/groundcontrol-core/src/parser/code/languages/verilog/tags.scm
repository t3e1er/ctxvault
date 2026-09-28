(module_declaration
  (module_header
    (simple_identifier) @name)) @definition.module

(task_declaration
  (task_body_declaration
    (task_identifier
      (task_identifier
        (simple_identifier) @name)))) @definition.function

(function_declaration
  (function_body_declaration
    (function_identifier
      (function_identifier
        (simple_identifier) @name)))) @definition.function