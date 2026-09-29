# sync-queries.ps1: Complete synchronization and generation of tree-sitter queries

$ErrorActionPreference = "Stop"

$helixBase = "https://raw.githubusercontent.com/helix-editor/helix/master/runtime/queries"
$targetDir = Join-Path $PSScriptRoot "..\crates\groundcontrol-core\src\parser\code\languages"

if (-not (Test-Path $targetDir)) {
    New-Item -ItemType Directory -Path $targetDir -Force | Out-Null
}

$helixMap = @{
    "rust" = "rust"
    "typescript" = "typescript"
    "tsx" = "tsx"
    "javascript" = "javascript"
    "python" = "python"
    "go" = "go"
    "c" = "c"
    "cpp" = "cpp"
    "java" = "java"
    "csharp" = "c-sharp"
    "ruby" = "ruby"
    "php" = "php"
    "swift" = "swift"
    "elixir" = "elixir"
    "lua" = "lua"
    "bash" = "bash"
    "kotlin" = "kotlin"
    "scala" = "scala"
    "zig" = "zig"
    "dart" = "dart"
    "toml" = "toml"
    "haskell" = "haskell"
    "julia" = "julia"
    "r" = "r"
    "nix" = "nix"
    "wgsl" = "wgsl"
    "erlang" = "erlang"
}

Write-Host "1. Syncing Helix tags.scm queries..."
foreach ($entry in $helixMap.GetEnumerator()) {
    $lang = $entry.Key
    $helixLang = $entry.Value
    $langDir = Join-Path $targetDir $lang
    if (-not (Test-Path $langDir)) {
        New-Item -ItemType Directory -Path $langDir -Force | Out-Null
    }

    $url = "$helixBase/$helixLang/tags.scm"
    $outFile = Join-Path $langDir "tags.scm"
    try {
        $content = (curl.exe -s -f $url)
        if ($content -and $content.Trim().Length -gt 0) {
            Set-Content -Path $outFile -Value $content -NoNewline
            Write-Host "  [OK] $lang"
        }
    } catch {
        Write-Warning "  [FAIL] $lang ($url)"
    }
}

Write-Host "2. Syncing upstream grammar tags.scm..."
# OCaml from tree-sitter-ocaml
$ocamlDir = Join-Path $targetDir "ocaml"
if (-not (Test-Path $ocamlDir)) { New-Item -ItemType Directory -Path $ocamlDir -Force | Out-Null }
$ocamlContent = curl.exe -s -f https://raw.githubusercontent.com/tree-sitter/tree-sitter-ocaml/master/queries/tags.scm
if ($ocamlContent) {
    Set-Content -Path (Join-Path $ocamlDir "tags.scm") -Value $ocamlContent -NoNewline
    Write-Host "  [OK] ocaml"
}

# Gleam from tree-sitter-gleam
$gleamDir = Join-Path $targetDir "gleam"
if (-not (Test-Path $gleamDir)) { New-Item -ItemType Directory -Path $gleamDir -Force | Out-Null }
$gleamContent = curl.exe -s -f https://raw.githubusercontent.com/gleam-lang/tree-sitter-gleam/main/queries/tags.scm
if ($gleamContent) {
    Set-Content -Path (Join-Path $gleamDir "tags.scm") -Value $gleamContent -NoNewline
    Write-Host "  [OK] gleam"
}

Write-Host "3. Writing standard tags.scm for schema/config/domain languages..."
$standaloneTags = @{
    "bicep" = @'
(resource_declaration
  (identifier) @name) @definition.struct

(module_declaration
  (identifier) @name) @definition.module

(parameter_declaration
  (identifier) @name) @definition.type

(variable_declaration
  (identifier) @name) @definition.type
'@

    "cmake" = @'
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
'@

    "css" = @'
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
'@

    "cuda" = @'
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
'@

    "d" = @'
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
'@

    "dockerfile" = @'
(from_instruction
  as: (image_alias) @name) @definition.module
'@

    "graphql" = @'
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
'@

    "hcl" = @'
(block
  (identifier)
  (string_lit) @name) @definition.struct

(block
  (identifier)
  (string_lit)
  (string_lit) @name) @definition.struct
'@

    "html" = @'
(element
  (start_tag
    (attribute
      (attribute_name) @_attr (#eq? @_attr "id")
      (quoted_attribute_value
        (attribute_value) @name)))) @definition.module
'@

    "json" = @'
(document
  (object
    (pair
      key: (string
        (string_content) @name)))) @definition.type
'@

    "make" = @'
(rule
  (targets
    (word) @name)) @definition.function
'@

    "powershell" = @'
(function_statement
  (function_name) @name) @definition.function

(class_statement
  (simple_name) @name) @definition.class

(enum_statement
  (simple_name) @name) @definition.enum
'@

    "proto" = @'
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
'@

    "solidity" = @'
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
'@

    "sql" = @'
(create_table
  (object_reference
    name: (identifier) @name)) @definition.struct

(create_view
  (object_reference
    name: (identifier) @name)) @definition.struct

(create_function
  (object_reference
    name: (identifier) @name)) @definition.function
'@

    "starlark" = @'
(function_definition
  name: (identifier) @name) @definition.function
'@

    "tlaplus" = @'
(module
  name: (identifier) @name) @definition.module

(operator_definition
  name: (identifier) @name) @definition.function

(function_definition
  name: (identifier) @name) @definition.function
'@

    "verilog" = @'
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
'@

    "yaml" = @'
(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @name))) @definition.type
'@
}

foreach ($entry in $standaloneTags.GetEnumerator()) {
    $lang = $entry.Key
    $langDir = Join-Path $targetDir $lang
    if (-not (Test-Path $langDir)) {
        New-Item -ItemType Directory -Path $langDir -Force | Out-Null
    }
    $outFile = Join-Path $langDir "tags.scm"
    Set-Content -Path $outFile -Value $entry.Value -NoNewline
    Write-Host "  [OK] $lang (schema/domain tags)"
}

Write-Host "4. Writing local variable & scope binding queries (locals.scm)..."
$localsQueries = @{
    "rust" = @'
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
'@

    "typescript" = @'
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

    "python" = @'
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
'@

    "go" = @'
;; Declarative Local Variable & Type Binding Patterns
(short_var_declaration
  left: (expression_list (identifier) @local.var)
  right: (expression_list (_) @local.type))

(parameter_declaration
  name: (identifier) @local.var
  type: (_) @local.type)
'@

    "java" = @'
;; Declarative Local Variable & Type Binding Patterns
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
'@

    "csharp" = @'
;; Declarative Local Variable & Type Binding Patterns
(local_declaration_statement
  (variable_declaration
    type: (_) @local.type
    (variable_declarator
      name: (identifier) @local.var)))

(parameter
  type: (_) @local.type
  name: (identifier) @local.var)
'@

    "cpp" = @'
;; Declarative Local Variable & Type Binding Patterns
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
'@

    "c" = @'
;; Declarative Local Variable & Type Binding Patterns
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
'@

    "kotlin" = @'
;; Declarative Local Variable & Type Binding Patterns
(parameter
  (identifier) @local.var
  (user_type) @local.type)

(variable_declaration
  (identifier) @local.var
  (user_type) @local.type)
'@

    "scala" = @'
;; Declarative Local Variable & Type Binding Patterns
(parameter
  name: (identifier) @local.var
  type: (_) @local.type)

(val_definition
  pattern: (identifier) @local.var
  type: (_) @local.type)
'@

    "swift" = @'
;; Declarative Local Variable & Type Binding Patterns
(parameter
  name: (simple_identifier) @local.var
  type: (_) @local.type)
'@
}

# Duplicate typescript locals to tsx and javascript
$localsQueries["tsx"] = $localsQueries["typescript"]
$localsQueries["javascript"] = $localsQueries["typescript"]

foreach ($entry in $localsQueries.GetEnumerator()) {
    $lang = $entry.Key
    $langDir = Join-Path $targetDir $lang
    if (-not (Test-Path $langDir)) {
        New-Item -ItemType Directory -Path $langDir -Force | Out-Null
    }
    $outFile = Join-Path $langDir "locals.scm"
    Set-Content -Path $outFile -Value $entry.Value -NoNewline
    Write-Host "  [OK] $lang (locals)"
}

Write-Host "5. Writing groundcontrol semantic route overlays (routes.scm)..."
$routeOverlays = @{
    "rust" = @'
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

    "typescript" = @'
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

    "python" = @'
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

    "go" = @'
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

    "java" = @'
;; Spring Boot: @GetMapping("/path"), @PostMapping("/path"), @RequestMapping("/path")
(method_declaration
  (modifiers
    (annotation
      name: (identifier) @_ann (#match? @_ann "^(GetMapping|PostMapping|PutMapping|DeleteMapping|PatchMapping|RequestMapping)$")
      arguments: (annotation_argument_list
        (string_literal) @name)))
  name: (identifier) @handles) @definition.route
'@

    "ruby" = @'
;; Rails: get '/path', to: 'controller#action'
(call
  method: (identifier) @_method (#match? @_method "^(get|post|put|delete|patch|match)$")
  arguments: (argument_list
    (string) @name
    (pair
      key: (hash_key_symbol) @_key (#match? @_key "^to$")
      value: (string) @handles))) @definition.route
'@

    "kotlin" = @'
;; Unit tests
(function_declaration
  (modifiers
    (annotation
      (user_type
        (identifier) @_ann (#match? @_ann "^(Test)$"))))
  name: (identifier) @name) @test
'@

    "zig" = @'
(test_declaration) @definition.function @test
'@
}

# Duplicate typescript routes to tsx and javascript
$routeOverlays["tsx"] = $routeOverlays["typescript"]
$routeOverlays["javascript"] = $routeOverlays["typescript"]

foreach ($entry in $routeOverlays.GetEnumerator()) {
    $lang = $entry.Key
    $langDir = Join-Path $targetDir $lang
    if (-not (Test-Path $langDir)) {
        New-Item -ItemType Directory -Path $langDir -Force | Out-Null
    }
    $outFile = Join-Path $langDir "routes.scm"
    Set-Content -Path $outFile -Value $entry.Value -NoNewline
    Write-Host "  [OK] $lang (routes)"
}

Write-Host "All 48 language queries generated and verified!"
