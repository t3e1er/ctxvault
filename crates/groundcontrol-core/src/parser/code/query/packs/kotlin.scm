;; Kotlin tags and definition query pack

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

;; Unit tests
(function_declaration
  (modifiers
    (annotation
      (user_type
        (identifier) @_ann (#match? @_ann "^(Test)$"))))
  name: (identifier) @name) @test
