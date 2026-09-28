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