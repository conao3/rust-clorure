(ns clrs.core$macros)

(def when clorure.bootstrap.primitives/when)
(def when-not clorure.bootstrap.primitives/when-not)
(def if-not clorure.bootstrap.primitives/if-not)

(defmacro defn [name args body]
  (list 'def name (list 'fn args body)))
