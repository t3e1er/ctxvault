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