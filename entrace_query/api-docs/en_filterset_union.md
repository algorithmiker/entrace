 en_filterset_union()
 input:
   filters: a list of filtersets, e. g
   {
     { type: "filterset",
       root: 1,
       items: {
         { type = "prim_list", value = {1,2,3}},
         { type = "rel", target = "a", relation = "EQ", value = "1", src = 0 },
       }
     }
     { type: "filterset",
       root: 1,
       items: {
         {type: "prim_list", value = {1,2,3} },
         {type: "rel", target = "b", relation = "EQ", value = "1", src = 0},
       }
     }
   }
 outputs: a filterset that matches an item if it is in any input filterset.
 This does NOT deduplicate any items, eg. for the given inputs, the result would be as follows.
 Note that en_materialize() MAY deduplicate, but there is no guarantee it will.
 { type: "filterset",
   root: 4,
   items: {
     { type = "prim_list", value = {1,2,3}},
     { type = "rel", target = "a", relation = "EQ", value = "1", src = 0 },
     { type: "prim_list", value = {1,2,3}},
     { type: "rel", target = "b", relation = "EQ", value = "1", src = 2 },
     { type: "union", srcs = { 1, 3 }}
 }

 Note: if you are unioning filters on the same source filterset, en_filter_any will likely
 be faster.