(ns clrs.core)

(def + clorure.bootstrap.primitives/+)
(def vector clorure.bootstrap.primitives/vector)
(def list clorure.bootstrap.primitives/list)
(def count clorure.bootstrap.primitives/count)
(def first clorure.bootstrap.primitives/first)
(def rest clorure.bootstrap.primitives/rest)
(def = clorure.bootstrap.primitives/=)
(def nil? clorure.bootstrap.primitives/nil?)
(def conj clorure.bootstrap.primitives/conj)
(def nth clorure.bootstrap.primitives/nth)
(def boolean clorure.bootstrap.primitives/boolean)
(def some? clorure.bootstrap.primitives/some?)
(def seq clorure.bootstrap.primitives/seq)
(def empty clorure.bootstrap.primitives/empty)
(def apply clorure.bootstrap.primitives/apply)
(def concat clorure.bootstrap.primitives/concat)

(def identity (fn [x] x))
(def inc (fn [x] (+ x 1)))
(def dec (fn [x] (+ x -1)))
(def empty? (fn [coll] (= 0 (count coll))))
(def not (fn [x] (if x false true)))
(def zero? (fn [x] (= x 0)))
(def second (fn [coll] (first (rest coll))))
(def third (fn [coll] (first (rest (rest coll)))))

(defn complement [f]
  (fn [& args]
    (not (apply f args))))

(defn partial [f & args]
  (fn [& more]
    (apply f (concat args more))))

(defn comp
  ([] identity)
  ([f] f)
  ([f g]
   (fn [& args]
     (f (apply g args)))))
