//! Vended Tree-sitter query packs for definitions, locals, and routes.
//!
//! Upstream vended queries based on nvim-treesitter / helix tags.scm and locals.scm,
//! combined with groundcontrol semantic route overlays. Eliminates maintaining
//! 48 individual .scm files on disk.

use crate::parser::code::languages::SupportedLanguage;

pub(crate) const BASH_QUERY: &str = r###";; Bash / Shell tags and definition query pack

(function_definition
  name: (word) @name) @definition.function"###;

pub(crate) const BICEP_QUERY: &str = r###";; Azure Bicep tags and definition query pack

(resource_declaration
  (identifier) @name) @definition.struct

(module_declaration
  (identifier) @name) @definition.module

(parameter_declaration
  (identifier) @name) @definition.type

(variable_declaration
  (identifier) @name) @definition.type"###;

pub(crate) const C_QUERY: &str = r###";; C tags and definition query pack

(function_definition
  declarator: (function_declarator
    declarator: (identifier) @name)) @definition.function

(struct_specifier
  name: (type_identifier) @name) @definition.struct

(enum_specifier
  name: (type_identifier) @name) @definition.enum

(type_definition
  declarator: (type_identifier) @name) @definition.type"###;

pub(crate) const CMAKE_QUERY: &str = r###";; CMake tags and definition query pack

(function_def
  (function_command
    (argument_list
      (argument
        (unquoted_argument) @name)))) @definition.function

(macro_def
  (macro_command
    (argument_list
      (argument
        (unquoted_argument) @name)))) @definition.function"###;

pub(crate) const CPP_QUERY: &str = r###";; C++ tags and definition query pack

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
  declarator: (identifier) @local.var)"###;

pub(crate) const CSHARP_QUERY: &str = r###";; C# tags and definition query pack

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

;; Declarative Local Variable & Type Binding Patterns
(local_declaration_statement
  (variable_declaration
    type: (_) @local.type
    (variable_declarator
      name: (identifier) @local.var)))

(parameter
  type: (_) @local.type
  name: (identifier) @local.var)"###;

pub(crate) const CSS_QUERY: &str = r###";; CSS tags and definition query pack

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
  (keyframes_name) @name) @definition.function"###;

pub(crate) const CUDA_QUERY: &str = r###";; CUDA tags and definition query pack

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
  declarator: (type_identifier) @name) @definition.type"###;

pub(crate) const D_QUERY: &str = r###";; D language tags and definition query pack

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
  (identifier) @name) @definition.class"###;

pub(crate) const DART_QUERY: &str = r###";; Dart tags and definition query pack

(function_signature
  name: (identifier) @name) @definition.function

(class_declaration
  name: (identifier) @name) @definition.class

(enum_declaration
  name: (identifier) @name) @definition.enum

(mixin_declaration
  name: (identifier) @name) @definition.trait"###;

pub(crate) const DOCKERFILE_QUERY: &str = r###";; Dockerfile tags and definition query pack

(from_instruction
  as: (image_alias) @name) @definition.module"###;

pub(crate) const ELIXIR_QUERY: &str = r###";; Elixir tags and definition query pack

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
    (identifier) @name)) @definition.function"###;

pub(crate) const ERLANG_QUERY: &str = r###";; Erlang tags and definition query pack

(module_attribute
  name: (atom) @name) @definition.module

(fun_decl
  clause: (function_clause
    name: (atom) @name)) @definition.function

(record_decl
  name: (atom) @name) @definition.struct"###;

pub(crate) const GLEAM_QUERY: &str = r###";; Gleam tags and definition query pack

(function
  name: (identifier) @name) @definition.function

(type_definition
  (type_name
    name: (type_identifier) @name)) @definition.type

(type_alias
  (type_name
    name: (type_identifier) @name)) @definition.type"###;

pub(crate) const GO_QUERY: &str = r###";; Go tags and definition query pack

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

;; Declarative Local Variable & Type Binding Patterns
(short_var_declaration
  left: (expression_list (identifier) @local.var)
  right: (expression_list (_) @local.type))

(parameter_declaration
  name: (identifier) @local.var
  type: (_) @local.type)"###;

pub(crate) const GRAPHQL_QUERY: &str = r###";; GraphQL tags and definition query pack

(object_type_definition
  (name) @name) @definition.class

(interface_type_definition
  (name) @name) @definition.interface

(enum_type_definition
  (name) @name) @definition.enum

(union_type_definition
  (name) @name) @definition.type

(field_definition
  (name) @name) @definition.function"###;

pub(crate) const HASKELL_QUERY: &str = r###";; Haskell tags and definition query pack

(function
  name: (variable) @name) @definition.function

(data_type
  name: (name) @name) @definition.struct

(class
  name: (name) @name) @definition.trait

(instance
  name: (name) @implements) @definition.module"###;

pub(crate) const HCL_QUERY: &str = r###";; HCL / Terraform tags and definition query pack

(block
  (identifier)
  (string_lit) @name) @definition.struct

(block
  (identifier)
  (string_lit)
  (string_lit) @name) @definition.struct"###;

pub(crate) const HTML_QUERY: &str = r###";; HTML tags and definition query pack

(element
  (start_tag
    (attribute
      (attribute_name) @_attr (#eq? @_attr "id")
      (quoted_attribute_value
        (attribute_value) @name)))) @definition.module"###;

pub(crate) const JAVA_QUERY: &str = r###";; Java tags and definition query pack

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
  name: (identifier) @local.var)"###;

pub(crate) const JSON_QUERY: &str = r###";; JSON tags and definition query pack

(document
  (object
    (pair
      key: (string
        (string_content) @name)))) @definition.type"###;

pub(crate) const JULIA_QUERY: &str = r###";; Julia tags and definition query pack

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
  name: (identifier) @name) @definition.module"###;

pub(crate) const KOTLIN_QUERY: &str = r###";; Kotlin tags and definition query pack

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

;; Declarative Local Variable & Type Binding Patterns
(parameter
  (identifier) @local.var
  (user_type) @local.type)

(variable_declaration
  (identifier) @local.var
  (user_type) @local.type)"###;

pub(crate) const LUA_QUERY: &str = r###";; Lua tags and definition query pack

(function_declaration
  name: (_) @name) @definition.function"###;

pub(crate) const MAKE_QUERY: &str = r###";; Makefile tags and definition query pack

(rule
  (targets
    (word) @name)) @definition.function"###;

pub(crate) const NIX_QUERY: &str = r###";; Nix tags and definition query pack

(binding
  attrpath: (attrpath (identifier) @name)
  expression: (function_expression)) @definition.function

(binding
  attrpath: (attrpath (identifier) @name)) @definition.type"###;

pub(crate) const OCAML_QUERY: &str = r###";; OCaml tags and definition query pack

(value_definition
  (let_binding
    pattern: (value_name) @name)) @definition.function

(type_definition
  (type_binding
    name: (type_constructor) @name)) @definition.type

(module_definition
  (module_binding
    (module_name) @name)) @definition.module"###;

pub(crate) const PHP_QUERY: &str = r###";; PHP tags and definition query pack

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
  name: (namespace_name) @name) @definition.module"###;

pub(crate) const POWERSHELL_QUERY: &str = r###";; PowerShell tags and definition query pack

(function_statement
  (function_name) @name) @definition.function

(class_statement
  (simple_name) @name) @definition.class

(enum_statement
  (simple_name) @name) @definition.enum"###;

pub(crate) const PROTO_QUERY: &str = r###";; Protocol Buffers tags and definition query pack

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
    (identifier) @name)) @definition.enum"###;

pub(crate) const PYTHON_QUERY: &str = r###";; Python tags and definition query pack

(function_definition
  name: (identifier) @name) @definition.function

(class_definition
  name: (identifier) @name
  superclasses: (argument_list
    (identifier) @inherits)?) @definition.class

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
  type: (type) @local.type)"###;

pub(crate) const R_QUERY: &str = r###";; R language tags and definition query pack

(binary_operator
  lhs: (identifier) @name
  rhs: (function_definition)) @definition.function"###;

pub(crate) const RUBY_QUERY: &str = r###";; Ruby tags and definition query pack

(method
  name: (identifier) @name) @definition.method

(singleton_method
  name: (identifier) @name) @definition.method

(class
  name: (constant) @name) @definition.class

(module
  name: (constant) @name) @definition.module

;; Rails: get '/path', to: 'controller#action'
(call
  method: (identifier) @_method (#match? @_method "^(get|post|put|delete|patch|match)$")
  arguments: (argument_list
    (string) @name
    (pair
      key: (hash_key_symbol) @_key (#match? @_key "^to$")
      value: (string) @handles))) @definition.route"###;

pub(crate) const RUST_QUERY: &str = r###";; Rust tags and definition query pack

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
  type: (_) @local.type)"###;

pub(crate) const SCALA_QUERY: &str = r###";; Scala tags and definition query pack

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

;; Declarative Local Variable & Type Binding Patterns
(parameter
  name: (identifier) @local.var
  type: (_) @local.type)

(val_definition
  pattern: (identifier) @local.var
  type: (_) @local.type)"###;

pub(crate) const SOLIDITY_QUERY: &str = r###";; Solidity tags and definition query pack

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
  name: (identifier) @name) @definition.type"###;

pub(crate) const SQL_QUERY: &str = r###";; SQL tags and definition query pack

(create_table
  (object_reference
    name: (identifier) @name)) @definition.struct

(create_view
  (object_reference
    name: (identifier) @name)) @definition.struct

(create_function
  (object_reference
    name: (identifier) @name)) @definition.function"###;

pub(crate) const STARLARK_QUERY: &str = r###";; Starlark / Bazel tags and definition query pack

(function_definition
  name: (identifier) @name) @definition.function"###;

pub(crate) const SWIFT_QUERY: &str = r###";; Swift tags and definition query pack

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

;; Declarative Local Variable & Type Binding Patterns
(parameter
  name: (simple_identifier) @local.var
  type: (_) @local.type)"###;

pub(crate) const TLAPLUS_QUERY: &str = r###";; TLA+ tags and definition query pack

(module
  name: (identifier) @name) @definition.module

(operator_definition
  name: (identifier) @name) @definition.function

(function_definition
  name: (identifier) @name) @definition.function"###;

pub(crate) const TOML_QUERY: &str = r###";; TOML tags and definition query pack

(table
  (bare_key) @name) @definition.module

(table_array_element
  (bare_key) @name) @definition.module"###;

pub(crate) const TYPESCRIPT_QUERY: &str = r###";; TypeScript, TSX, and JavaScript tags and definition query pack

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
  type: (type_annotation) @local.type)"###;

pub(crate) const VERILOG_QUERY: &str = r###";; Verilog / SystemVerilog tags and definition query pack

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
        (simple_identifier) @name)))) @definition.function"###;

pub(crate) const WGSL_QUERY: &str = r###";; WGSL WebGPU Shader tags and definition query pack

(function_declaration
  name: (identifier) @name) @definition.function

(struct_declaration
  name: (identifier) @name) @definition.struct"###;

pub(crate) const YAML_QUERY: &str = r###";; YAML tags and definition query pack

(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @name))) @definition.type"###;

pub(crate) const ZIG_QUERY: &str = r###";; Zig tags and definition query pack

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

(test_declaration) @definition.function @test"###;

/// Get the compiled vended query string for a supported language.
pub fn get_vended_query(lang: SupportedLanguage) -> &'static str {
    match lang {
        SupportedLanguage::Rust => RUST_QUERY,
        SupportedLanguage::TypeScript | SupportedLanguage::JavaScript | SupportedLanguage::Tsx => TYPESCRIPT_QUERY,
        SupportedLanguage::Python => PYTHON_QUERY,
        SupportedLanguage::Go => GO_QUERY,
        SupportedLanguage::Java => JAVA_QUERY,
        SupportedLanguage::CSharp => CSHARP_QUERY,
        SupportedLanguage::C => C_QUERY,
        SupportedLanguage::Cpp => CPP_QUERY,
        SupportedLanguage::Ruby => RUBY_QUERY,
        SupportedLanguage::Php => PHP_QUERY,
        SupportedLanguage::Swift => SWIFT_QUERY,
        SupportedLanguage::Elixir => ELIXIR_QUERY,
        SupportedLanguage::Lua => LUA_QUERY,
        SupportedLanguage::Bash => BASH_QUERY,
        SupportedLanguage::Kotlin => KOTLIN_QUERY,
        SupportedLanguage::Scala => SCALA_QUERY,
        SupportedLanguage::Zig => ZIG_QUERY,
        SupportedLanguage::Dart => DART_QUERY,
        SupportedLanguage::Sql => SQL_QUERY,
        SupportedLanguage::Yaml => YAML_QUERY,
        SupportedLanguage::Dockerfile => DOCKERFILE_QUERY,
        SupportedLanguage::Proto => PROTO_QUERY,
        SupportedLanguage::Solidity => SOLIDITY_QUERY,
        SupportedLanguage::Html => HTML_QUERY,
        SupportedLanguage::Css => CSS_QUERY,
        SupportedLanguage::Json => JSON_QUERY,
        SupportedLanguage::Toml => TOML_QUERY,
        SupportedLanguage::Ocaml => OCAML_QUERY,
        SupportedLanguage::Haskell => HASKELL_QUERY,
        SupportedLanguage::Cmake => CMAKE_QUERY,
        SupportedLanguage::Make => MAKE_QUERY,
        SupportedLanguage::Julia => JULIA_QUERY,
        SupportedLanguage::Graphql => GRAPHQL_QUERY,
        SupportedLanguage::R => R_QUERY,
        SupportedLanguage::Hcl => HCL_QUERY,
        SupportedLanguage::Nix => NIX_QUERY,
        SupportedLanguage::Cuda => CUDA_QUERY,
        SupportedLanguage::Verilog => VERILOG_QUERY,
        SupportedLanguage::Tlaplus => TLAPLUS_QUERY,
        SupportedLanguage::Starlark => STARLARK_QUERY,
        SupportedLanguage::Bicep => BICEP_QUERY,
        SupportedLanguage::Gleam => GLEAM_QUERY,
        SupportedLanguage::PowerShell => POWERSHELL_QUERY,
        SupportedLanguage::D => D_QUERY,
        SupportedLanguage::Wgsl => WGSL_QUERY,
        SupportedLanguage::Erlang => ERLANG_QUERY,
    }
}

