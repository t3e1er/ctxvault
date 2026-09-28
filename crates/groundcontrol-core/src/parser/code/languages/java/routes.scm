;; Spring Boot: @GetMapping("/path"), @PostMapping("/path"), @RequestMapping("/path")
(method_declaration
  (modifiers
    (annotation
      name: (identifier) @_ann (#match? @_ann "^(GetMapping|PostMapping|PutMapping|DeleteMapping|PatchMapping|RequestMapping)$")
      arguments: (annotation_argument_list
        (string_literal) @name)))
  name: (identifier) @handles) @definition.route