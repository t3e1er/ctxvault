# populate-all-queries.ps1: Populate complete, robust tags.scm, locals.scm, and routes.scm across all 48 languages.

$ErrorActionPreference = "Stop"
$queriesDir = Join-Path $PSScriptRoot "..\crates\groundcontrol-core\src\parser\code\languages"

if (-not (Test-Path $queriesDir)) {
    New-Item -ItemType Directory -Path $queriesDir -Force | Out-Null
}

function Save-QueryPack($lang, $tags, $locals, $routes) {
    $dir = Join-Path $queriesDir $lang
    if (-not (Test-Path $dir)) {
        New-Item -ItemType Directory -Path $dir -Force | Out-Null
    }

    $tagsVal = if ($tags) { $tags.Trim() } else { "" }
    $localsVal = if ($locals) { $locals.Trim() } else { "" }
    $routesVal = if ($routes) { $routes.Trim() } else { "" }
    Set-Content -Path (Join-Path $dir "tags.scm") -Value $tagsVal -NoNewline
    Set-Content -Path (Join-Path $dir "locals.scm") -Value $localsVal -NoNewline
    Set-Content -Path (Join-Path $dir "routes.scm") -Value $routesVal -NoNewline
    Write-Host "  [OK] $lang"
}

Write-Host "Populating complete query packs for all 48 languages..."

# 1. Rust
Save-QueryPack "rust" @'
(function_item
  name: (identifier) @name) @definition.function

(function_signature_item
  name: (identifier) @name) @definition.function

(struct_item
  name: (type_identifier) @name) @definition.struct

(union_item
  name: (type_identifier) @name) @definition.struct

(enum_item
  name: (type_identifier) @name) @definition.enum

(trait_item
  name: (type_identifier) @name) @definition.trait

(mod_item
  name: (identifier) @name) @definition.module

(type_item
  name: (type_identifier) @name) @definition.type

(impl_item
  trait: (type_identifier)? @implements
  type: (_) @name) @definition.module
'@ @'
;; Declarative Local Variable & Type Binding Patterns
(let_declaration
  pattern: (identifier) @local.var
  type: (_) @local.type)

(let_declaration
  pattern: (identifier) @local.var
  value: (call_expression
    function: (scoped_identifier
      path: (identifier) @local.type)))

(let_declaration
  pattern: (identifier) @local.var
  value: (struct_expression
    name: (type_identifier) @local.type))

(parameter
  pattern: (identifier) @local.var
  type: (_) @local.type)
'@ @'
;; Declarative Route Query Patterns (Axum & Actix)
(call_expression
  function: (field_expression field: (field_identifier) @_route_fn (#eq? @_route_fn "route"))
  arguments: (arguments
    (string_literal) @name
    (call_expression
      arguments: (arguments (identifier) @handles)))) @definition.route

(call_expression
  function: (field_expression field: (field_identifier) @_route_fn (#eq? @_route_fn "route"))
  arguments: (arguments
    (string_literal) @name
    (identifier) @handles)) @definition.route

(attribute_item
  (attribute
    (identifier) @_method (#match? @_method "^(get|post|put|delete|patch|head)$")
    (token_tree (string_literal) @name))) @definition.route
'@

# 2. TypeScript
$tsTags = @'
(function_declaration
  name: (identifier) @name) @definition.function

(method_definition
  name: (property_identifier) @name) @definition.method

(class_declaration
  name: (type_identifier) @name) @definition.class

(class_declaration
  name: (type_identifier) @name
  (class_heritage
    (extends_clause
      value: (_) @inherits))) @definition.class

(class_declaration
  name: (type_identifier) @name
  (class_heritage
    (implements_clause
      (type_identifier) @implements))) @definition.class

(interface_declaration
  name: (type_identifier) @name) @definition.interface

(interface_declaration
  name: (type_identifier) @name
  (extends_type_clause
    (type_identifier) @inherits)) @definition.interface

(type_alias_declaration
  name: (type_identifier) @name) @definition.type

(enum_declaration
  name: (identifier) @name) @definition.enum

(module
  name: (identifier) @name) @definition.module

(internal_module
  name: (identifier) @name) @definition.module
'@

$tsLocals = @'
;; Declarative Local Variable & Type Binding Patterns
(variable_declarator
  name: (identifier) @local.var
  type: (type_annotation) @local.type)

(variable_declarator
  name: (identifier) @local.var
  value: (new_expression
    constructor: (identifier) @local.type))

(required_parameter
  pattern: (identifier) @local.var
  type: (type_annotation) @local.type)

(optional_parameter
  pattern: (identifier) @local.var
  type: (type_annotation) @local.type)
'@

$tsRoutes = @'
;; Declarative Route Query Patterns (Express / Fastify / Koa)
(call_expression
  function: (member_expression
    property: (property_identifier) @_method (#match? @_method "^(get|post|put|delete|patch|all|options|head)$"))
  arguments: (arguments
    (string) @name
    [(identifier) (arrow_function) (function_expression)] @handles)) @definition.route

(call_expression
  function: (member_expression
    property: (property_identifier) @_method (#match? @_method "^(get|post|put|delete|patch|all|options|head)$"))
  arguments: (arguments
    (string) @name)) @definition.route
'@

Save-QueryPack "typescript" $tsTags $tsLocals $tsRoutes
Save-QueryPack "tsx" $tsTags $tsLocals $tsRoutes
Save-QueryPack "javascript" $tsTags $tsLocals $tsRoutes

# 3. Python
Save-QueryPack "python" @'
(function_definition
  name: (identifier) @name) @definition.function

(class_definition
  name: (identifier) @name
  superclasses: (argument_list
    (identifier) @inherits)?) @definition.class
'@ @'
;; Declarative Local Variable & Type Binding Patterns
(assignment
  left: (identifier) @local.var
  type: (type) @local.type)

(assignment
  left: (identifier) @local.var
  right: (call
    function: (identifier) @local.type))

(typed_parameter
  (identifier) @local.var
  type: (type) @local.type)

(typed_default_parameter
  name: (identifier) @local.var
  type: (type) @local.type)
'@ @'
;; FastAPI / Flask: @app.get("/path"), @app.route("/path")
(decorated_definition
  (decorator
    (call
      function: (attribute
        attribute: (identifier) @_method (#match? @_method "^(get|post|put|delete|patch|options|head|route)$"))
      arguments: (argument_list
        (string) @name)))
  definition: (function_definition
    name: (identifier) @handles)) @definition.route
'@

# 4. Go
Save-QueryPack "go" @'
(function_declaration
  name: (identifier) @name) @definition.function

(method_declaration
  name: (field_identifier) @name) @definition.method

(type_spec
  name: (type_identifier) @name
  type: (struct_type)) @definition.struct

(type_spec
  name: (type_identifier) @name
  type: (struct_type
    (field_declaration_list
      (field_declaration
        type: (type_identifier) @struct_embeds
        !name)))) @definition.struct

(type_spec
  name: (type_identifier) @name
  type: (interface_type)) @definition.interface

(type_spec
  name: (type_identifier) @name) @definition.type
'@ @'
;; Declarative Local Variable & Type Binding Patterns
(short_var_declaration
  left: (expression_list (identifier) @local.var)
  right: (expression_list (_) @local.type))

(parameter_declaration
  name: (identifier) @local.var
  type: (_) @local.type)
'@ @'
;; Declarative Route Query Patterns (Gin, Echo, Chi, net/http)
(call_expression
  function: (selector_expression
    field: (field_identifier) @_method (#match? @_method "^(GET|POST|PUT|DELETE|PATCH|Any|Handle|HandleFunc)$"))
  arguments: (argument_list
    [(interpreted_string_literal) (raw_string_literal)] @name
    (identifier) @handles)) @definition.route

(call_expression
  function: (selector_expression
    field: (field_identifier) @_method (#match? @_method "^(GET|POST|PUT|DELETE|PATCH|Any|Handle|HandleFunc)$"))
  arguments: (argument_list
    [(interpreted_string_literal) (raw_string_literal)] @name)) @definition.route
'@

# 5. C
Save-QueryPack "c" @'
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @name)) @definition.function

(struct_specifier
  name: (type_identifier) @name) @definition.struct

(enum_specifier
  name: (type_identifier) @name) @definition.enum

(type_definition
  declarator: (type_identifier) @name) @definition.type
'@ @'
(declaration
  type: (_) @local.type
  declarator: (identifier) @local.var)

(declaration
  type: (_) @local.type
  declarator: (init_declarator
    declarator: (identifier) @local.var))

(parameter_declaration
  type: (_) @local.type
  declarator: (identifier) @local.var)
'@ ""

# 6. C++
Save-QueryPack "cpp" @'
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @name)) @definition.function

(function_definition
  declarator: (function_declarator
    declarator: (field_identifier) @name)) @definition.method

(class_specifier
  name: (type_identifier) @name
  (base_class_clause (type_identifier) @inherits)?) @definition.class

(struct_specifier
  name: (type_identifier) @name) @definition.struct

(enum_specifier
  name: (type_identifier) @name) @definition.enum

(namespace_definition
  name: (namespace_identifier) @name) @definition.module

(type_definition
  declarator: (type_identifier) @name) @definition.type

(alias_declaration
  name: (type_identifier) @name) @definition.type
'@ @'
(declaration
  type: (_) @local.type
  declarator: (identifier) @local.var)

(declaration
  type: (_) @local.type
  declarator: (init_declarator
    declarator: (identifier) @local.var))

(parameter_declaration
  type: (_) @local.type
  declarator: (identifier) @local.var)
'@ ""

# 7. Java
Save-QueryPack "java" @'
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
'@ @'
(local_variable_declaration
  type: (_) @local.type
  declarator: (variable_declarator
    name: (identifier) @local.var))

(variable_declarator
  name: (identifier) @local.var
  value: (object_creation_expression
    type: (_) @local.type))

(formal_parameter
  type: (_) @local.type
  name: (identifier) @local.var)
'@ @'
;; Spring Boot: @GetMapping("/path"), @PostMapping("/path"), @RequestMapping("/path")
(method_declaration
  (modifiers
    (annotation
      name: (identifier) @_ann (#match? @_ann "^(GetMapping|PostMapping|PutMapping|DeleteMapping|PatchMapping|RequestMapping)$")
      arguments: (annotation_argument_list
        (string_literal) @name)))
  name: (identifier) @handles) @definition.route
'@

# 8. C#
Save-QueryPack "csharp" @'
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
'@ @'
(local_declaration_statement
  (variable_declaration
    type: (_) @local.type
    (variable_declarator
      name: (identifier) @local.var)))

(parameter
  type: (_) @local.type
  name: (identifier) @local.var)
'@ ""

# 9. Ruby
Save-QueryPack "ruby" @'
(method
  name: (identifier) @name) @definition.method

(singleton_method
  name: (identifier) @name) @definition.method

(class
  name: (constant) @name) @definition.class

(module
  name: (constant) @name) @definition.module
'@ "" @'
;; Rails: get '/path', to: 'controller#action'
(call
  method: (identifier) @_method (#match? @_method "^(get|post|put|delete|patch|match)$")
  arguments: (argument_list
    (string) @name
    (pair
      key: (hash_key_symbol) @_key (#match? @_key "^to$")
      value: (string) @handles))) @definition.route
'@

# 10. PHP
Save-QueryPack "php" @'
(function_definition
  name: (name) @name) @definition.function

(method_declaration
  name: (name) @name) @definition.method

(class_declaration
  name: (name) @name) @definition.class

(interface_declaration
  name: (name) @name) @definition.interface

(trait_declaration
  name: (name) @name) @definition.trait

(enum_declaration
  name: (name) @name) @definition.enum

(namespace_definition
  name: (namespace_name) @name) @definition.module
'@ "" ""

# 11. Swift
Save-QueryPack "swift" @'
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
'@ @'
(parameter
  name: (simple_identifier) @local.var
  type: (_) @local.type)
'@ ""

# 12. Elixir
Save-QueryPack "elixir" @'
(call
  target: (identifier) @_macro (#match? @_macro "^(defmodule)$")
  (arguments
    (alias) @name)) @definition.module

(call
  target: (identifier) @_macro (#match? @_macro "^(defprotocol)$")
  (arguments
    (alias) @name)) @definition.interface

(call
  target: (identifier) @_macro (#match? @_macro "^(def|defp)$")
  (arguments
    (call
      target: (identifier) @name))) @definition.function

(call
  target: (identifier) @_macro (#match? @_macro "^(def|defp)$")
  (arguments
    (identifier) @name)) @definition.function
'@ "" ""

# 13. Lua
Save-QueryPack "lua" @'
(function_declaration
  name: (_) @name) @definition.function
'@ "" ""

# 14. Bash
Save-QueryPack "bash" @'
(function_definition
  name: (word) @name) @definition.function
'@ "" ""

# 15. Kotlin
Save-QueryPack "kotlin" @'
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
'@ @'
(parameter
  (identifier) @local.var
  (user_type) @local.type)

(variable_declaration
  (identifier) @local.var
  (user_type) @local.type)
'@ @'
;; Unit tests
(function_declaration
  (modifiers
    (annotation
      (user_type
        (identifier) @_ann (#match? @_ann "^(Test)$"))))
  name: (identifier) @name) @test
'@

# 16. Scala
Save-QueryPack "scala" @'
(function_definition
  name: (identifier) @name) @definition.function

(function_declaration
  name: (identifier) @name) @definition.function

(class_definition
  name: (identifier) @name
  extend: (extends_clause
    type: (type_identifier) @inherits)?) @definition.class

(object_definition
  name: (identifier) @name
  extend: (extends_clause
    type: (type_identifier) @inherits)?) @definition.class

(trait_definition
  name: (identifier) @name
  extend: (extends_clause
    type: (type_identifier) @inherits)?) @definition.trait
'@ @'
(parameter
  name: (identifier) @local.var
  type: (_) @local.type)

(val_definition
  pattern: (identifier) @local.var
  type: (_) @local.type)
'@ ""

# 17. Zig
Save-QueryPack "zig" @'
(function_declaration
  name: (identifier) @name) @definition.function

(variable_declaration
  (identifier) @name
  (struct_declaration)) @definition.struct

(variable_declaration
  (identifier) @name
  (enum_declaration)) @definition.enum

(variable_declaration
  (identifier) @name
  (union_declaration)) @definition.struct
'@ "" @'
(test_declaration) @definition.function @test
'@

# 18. Dart
Save-QueryPack "dart" @'
(function_signature
  name: (identifier) @name) @definition.function

(class_declaration
  name: (identifier) @name) @definition.class

(enum_declaration
  name: (identifier) @name) @definition.enum

(mixin_declaration
  name: (identifier) @name) @definition.trait
'@ "" ""

# 19. SQL
Save-QueryPack "sql" @'
(create_table
  (object_reference
    name: (identifier) @name)) @definition.struct

(create_view
  (object_reference
    name: (identifier) @name)) @definition.struct

(create_function
  (object_reference
    name: (identifier) @name)) @definition.function
'@ "" ""

# 20. YAML
Save-QueryPack "yaml" @'
(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @name))) @definition.type
'@ "" ""

# 21. Dockerfile
Save-QueryPack "dockerfile" @'
(from_instruction
  as: (image_alias) @name) @definition.module
'@ "" ""

# 22. Proto
Save-QueryPack "proto" @'
(service
  (service_name
    (identifier) @name)) @definition.interface

(rpc
  (rpc_name
    (identifier) @name)) @definition.function

(message
  (message_name
    (identifier) @name)) @definition.struct

(enum
  (enum_name
    (identifier) @name)) @definition.enum
'@ "" ""

# 23. Solidity
Save-QueryPack "solidity" @'
(contract_declaration
  name: (identifier) @name
  (inheritance_specifier
    ancestor: (user_defined_type (identifier) @inherits))?) @definition.class

(interface_declaration
  name: (identifier) @name) @definition.interface

(library_declaration
  name: (identifier) @name) @definition.module

(function_definition
  name: (identifier) @name) @definition.function

(struct_declaration
  (identifier) @name) @definition.struct

(enum_declaration
  (identifier) @name) @definition.enum

(event_definition
  name: (identifier) @name) @definition.type

(error_declaration
  name: (identifier) @name) @definition.type
'@ "" ""

# 24. HTML
Save-QueryPack "html" @'
(element
  (start_tag
    (attribute
      (attribute_name) @_attr (#eq? @_attr "id")
      (quoted_attribute_value
        (attribute_value) @name)))) @definition.module
'@ "" ""

# 25. CSS
Save-QueryPack "css" @'
(rule_set
  (selectors
    (class_selector
      (class_name
        (identifier) @name)))) @definition.class

(rule_set
  (selectors
    (id_selector
      (id_name) @name))) @definition.class

(keyframes_statement
  (keyframes_name) @name) @definition.function
'@ "" ""

# 26. JSON
Save-QueryPack "json" @'
(document
  (object
    (pair
      key: (string
        (string_content) @name)))) @definition.type
'@ "" ""

# 27. TOML
Save-QueryPack "toml" @'
(table
  (bare_key) @name) @definition.module

(table_array_element
  (bare_key) @name) @definition.module
'@ "" ""

# 28. OCaml
Save-QueryPack "ocaml" @'
(value_definition
  (let_binding
    pattern: (value_name) @name)) @definition.function

(type_definition
  (type_binding
    name: (type_constructor) @name)) @definition.type

(module_definition
  (module_binding
    (module_name) @name)) @definition.module
'@ "" ""

# 29. Haskell
Save-QueryPack "haskell" @'
(function
  name: (variable) @name) @definition.function

(data_type
  name: (name) @name) @definition.struct

(class
  name: (name) @name) @definition.trait

(instance
  name: (name) @implements) @definition.module
'@ "" ""

# 30. CMake
Save-QueryPack "cmake" @'
(function_def
  (function_command
    (argument_list
      (argument
        (unquoted_argument) @name)))) @definition.function

(macro_def
  (macro_command
    (argument_list
      (argument
        (unquoted_argument) @name)))) @definition.function
'@ "" ""

# 31. Make
Save-QueryPack "make" @'
(rule
  (targets
    (word) @name)) @definition.function
'@ "" ""

# 32. Julia
Save-QueryPack "julia" @'
(function_definition
  (signature
    (call_expression
      (identifier) @name))) @definition.function

(struct_definition
  (type_head
    (identifier) @name)) @definition.struct

(abstract_definition
  (type_head
    (identifier) @name)) @definition.type

(module_definition
  name: (identifier) @name) @definition.module
'@ "" ""

# 33. GraphQL
Save-QueryPack "graphql" @'
(object_type_definition
  (name) @name) @definition.class

(interface_type_definition
  (name) @name) @definition.interface

(enum_type_definition
  (name) @name) @definition.enum

(union_type_definition
  (name) @name) @definition.type

(field_definition
  (name) @name) @definition.function
'@ "" ""

# 34. R
Save-QueryPack "r" @'
(binary_operator
  lhs: (identifier) @name
  rhs: (function_definition)) @definition.function
'@ "" ""

# 35. HCL
Save-QueryPack "hcl" @'
(block
  (identifier)
  (string_lit) @name) @definition.struct

(block
  (identifier)
  (string_lit)
  (string_lit) @name) @definition.struct
'@ "" ""

# 36. Nix
Save-QueryPack "nix" @'
(binding
  attrpath: (attrpath (identifier) @name)
  expression: (function_expression)) @definition.function

(binding
  attrpath: (attrpath (identifier) @name)) @definition.type
'@ "" ""

# 37. CUDA
Save-QueryPack "cuda" @'
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @name)) @definition.function

(struct_specifier
  name: (type_identifier) @name) @definition.struct

(class_specifier
  name: (type_identifier) @name) @definition.class

(enum_specifier
  name: (type_identifier) @name) @definition.enum

(type_definition
  declarator: (type_identifier) @name) @definition.type
'@ "" ""

# 38. Verilog
Save-QueryPack "verilog" @'
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
'@ "" ""

# 39. TLA+
Save-QueryPack "tlaplus" @'
(module
  name: (identifier) @name) @definition.module

(operator_definition
  name: (identifier) @name) @definition.function

(function_definition
  name: (identifier) @name) @definition.function
'@ "" ""

# 40. Starlark
Save-QueryPack "starlark" @'
(function_definition
  name: (identifier) @name) @definition.function
'@ "" ""

# 41. Bicep
Save-QueryPack "bicep" @'
(resource_declaration
  (identifier) @name) @definition.struct

(module_declaration
  (identifier) @name) @definition.module

(parameter_declaration
  (identifier) @name) @definition.type

(variable_declaration
  (identifier) @name) @definition.type
'@ "" ""

# 42. Gleam
Save-QueryPack "gleam" @'
(function
  name: (identifier) @name) @definition.function

(type_definition
  (type_name
    name: (type_identifier) @name)) @definition.type

(type_alias
  (type_name
    name: (type_identifier) @name)) @definition.type
'@ "" ""

# 43. PowerShell
Save-QueryPack "powershell" @'
(function_statement
  (function_name) @name) @definition.function

(class_statement
  (simple_name) @name) @definition.class

(enum_statement
  (simple_name) @name) @definition.enum
'@ "" ""

# 44. D
Save-QueryPack "d" @'
(function_declaration
  (identifier) @name) @definition.function

(class_declaration
  (identifier) @name) @definition.class

(interface_declaration
  (identifier) @name) @definition.interface

(struct_declaration
  (identifier) @name) @definition.struct

(enum_declaration
  (identifier) @name) @definition.enum

(template_declaration
  (identifier) @name) @definition.class
'@ "" ""

# 45. WGSL
Save-QueryPack "wgsl" @'
(function_declaration
  name: (identifier) @name) @definition.function

(struct_declaration
  name: (identifier) @name) @definition.struct
'@ "" ""

# 46. Erlang
Save-QueryPack "erlang" @'
(module_attribute
  name: (atom) @name) @definition.module

(fun_decl
  clause: (function_clause
    name: (atom) @name)) @definition.function

(record_decl
  name: (atom) @name) @definition.struct
'@ "" ""

Write-Host "Done populating all 48 language query packs!"
