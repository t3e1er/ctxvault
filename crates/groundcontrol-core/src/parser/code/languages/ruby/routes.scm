;; Rails: get '/path', to: 'controller#action'
(call
  method: (identifier) @_method (#match? @_method "^(get|post|put|delete|patch|match)$")
  arguments: (argument_list
    (string) @name
    (pair
      key: (hash_key_symbol) @_key (#match? @_key "^to$")
      value: (string) @handles))) @definition.route