check("the base builds a subclass's row as it", Article.find_by(title: "f").class.name, "Feature")
check("the hook runs once, hydrated, through the base", HookProbe.base_find_by, [1, "f"])
check("the hook runs once, hydrated, on the subclass", HookProbe.first, [1, "f"])
check("the hook runs once, hydrated, on a projection", HookProbe.select_first, [1, "f"])
puts "hooked sti queries passed"
