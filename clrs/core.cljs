(ns clrs.core)

(def + clorure.bootstrap.primitives/+)
(def vector clorure.bootstrap.primitives/vector)
(def list clorure.bootstrap.primitives/list)
(def count clorure.bootstrap.primitives/count)
(def first clorure.bootstrap.primitives/first)
(def rest clorure.bootstrap.primitives/rest)
(def = clorure.bootstrap.primitives/=)
(def nil? clorure.bootstrap.primitives/nil?)

(def identity (fn [x] x))
(def inc (fn [x] (+ x 1)))
(def empty? (fn [coll] (= 0 (count coll))))
