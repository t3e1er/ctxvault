(function
  name: (variable) @name) @definition.function

(data_type
  name: (name) @name) @definition.struct

(class
  name: (name) @name) @definition.trait

(instance
  name: (name) @implements) @definition.module