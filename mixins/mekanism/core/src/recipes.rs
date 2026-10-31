// Item-to-item machine recipes (mekanism.api.recipes.ItemStackToItemStackRecipe), read from this
// repository's generated data by tools/gen_recipe_table.py into recipe_table.rs.

/// What a recipe takes: an item id, or a tag (`c:ores/osmium`, without `#`), or any of several.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ingredient {
    Item(&'static str),
    Tag(&'static str),
    Any(&'static [Ingredient]),
}

impl Ingredient {
    /// Whether `item` matches; `in_tag(tag, item)` answers tag membership from the game's tags.
    pub fn matches(&self, item: &str, in_tag: &dyn Fn(&str, &str) -> bool) -> bool {
        match self {
            Ingredient::Item(id) => *id == item,
            Ingredient::Tag(tag) => in_tag(tag, item),
            Ingredient::Any(all) => all.iter().any(|i| i.matches(item, in_tag)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemRecipe {
    /// The recipe's id (`mekanism:processing/osmium/dust/from_ore`).
    pub id: &'static str,
    pub input: Ingredient,
    pub input_count: u32,
    pub output: &'static str,
    pub output_count: u32,
}

/// The first recipe of `recipes` that `item` (at least `count` of it) matches, as Mekanism's input
/// cache finds the first match.
pub fn find<'a>(
    recipes: &'a [ItemRecipe],
    item: &str,
    count: u32,
    in_tag: &dyn Fn(&str, &str) -> bool,
) -> Option<&'a ItemRecipe> {
    recipes
        .iter()
        .find(|r| count >= r.input_count && r.input.matches(item, in_tag))
}

/// The recipe types this port runs, with their tables.
pub fn table(kind: &str) -> &'static [ItemRecipe] {
    match kind {
        "enriching" => crate::recipe_table::ENRICHING,
        "crushing" => crate::recipe_table::CRUSHING,
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(tag: &str, item: &str) -> bool {
        tag == "c:ores/osmium" && item == "mekanism:osmium_ore"
    }

    #[test]
    fn osmium_ore_enriches_into_two_dusts() {
        let r = find(table("enriching"), "mekanism:osmium_ore", 1, &tags).expect("a recipe");
        assert_eq!((r.output, r.output_count), ("mekanism:dust_osmium", 2));
    }

    #[test]
    fn too_few_items_or_an_unknown_item_find_nothing() {
        assert!(find(table("enriching"), "minecraft:dirt", 64, &tags).is_none());
        let custom = [ItemRecipe {
            id: "t:x",
            input: Ingredient::Item("a"),
            input_count: 3,
            output: "b",
            output_count: 1,
        }];
        assert!(find(&custom, "a", 2, &tags).is_none());
        assert!(find(&custom, "a", 3, &tags).is_some());
    }

    #[test]
    fn the_tables_hold_the_generated_recipes() {
        assert!(
            table("enriching").len() > 50,
            "{}",
            table("enriching").len()
        );
        assert!(table("crushing").len() > 50, "{}", table("crushing").len());
    }
}
