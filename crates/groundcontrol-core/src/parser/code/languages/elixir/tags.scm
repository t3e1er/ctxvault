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