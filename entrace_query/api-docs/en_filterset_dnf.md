 en_filterset_dnf()
 input:
   filters: a list of list of filter descriptions, which is interpreted as a DNF clause list.
   (this example would be (a=1 AND c=0) OR (b=1)
   {
     {
       { target = "a", relation = "EQ", value = "1", src = 0 },
       { target = "c", relation = "EQ", value = "0", src = 0 },
     }
     {
       { target = "b", relation = "EQ", value = "1", src = 0},
     }
   }
   source: a filterset
 outputs: a filterset that matches an item if satisfies either of the AND clauses
 { type: "filterset",
   root: 1,
   items: {
     { type = "prim_list", value = {1,2,3}},
     { type = "rel_dnf", src = 0,
       clauses = {
         {
           { target = "a", relation = "EQ", value = "1", src = 0 },
           { target = "c", relation = "EQ", value = "0", src = 0 },
         }
         {
           { target = "b", relation = "EQ", value = "1", src = 0},
         }
       }
     }
 }