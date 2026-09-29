;; XSLT template and function declarations
(element
  (STag
    (Name) @_tag (#eq? @_tag "xsl:template")
    (Attribute
      (Name) @_attr (#match? @_attr "^(name|match)$")
      (AttValue) @name))) @definition.function

(element
  (STag
    (Name) @_tag (#eq? @_tag "xsl:variable")
    (Attribute
      (Name) @_attr (#eq? @_attr "name")
      (AttValue) @name))) @definition.variable

(element
  (EmptyElemTag
    (Name) @_tag (#match? @_tag "^(xsl:include|xsl:import)$")
    (Attribute
      (Name) @_attr (#eq? @_attr "href")
      (AttValue) @name))) @reference.import

(element
  (EmptyElemTag
    (Name) @_tag (#match? @_tag "^(xsl:call-template)$")
    (Attribute
      (Name) @_attr (#eq? @_attr "name")
      (AttValue) @name))) @reference.call

;; Generic XML element definitions
(element
  (STag
    (Name) @name)) @definition.struct
