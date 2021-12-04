mod pokemon_csv;
mod db;
use db::*;
use futures::{stream::FuturesUnordered, StreamExt};
use pokemon_csv::*;
use sqlx::{mysql::MySqlPoolOptions};
use std::{collections::HashMap,env, time::Duration};
use color_eyre::{eyre, eyre::WrapErr, Section};
use indicatif::ProgressBar;

#[tokio::main]
async fn main() -> eyre::Result<()> {
    color_eyre::install()?;
    let database_url = env::var("DATABASE_URL")
        .wrap_err("Must have a DATABASE_URL set")
        .suggestion("Run `pscale connect <database> <branch>` to get a connection")?;



    let pool = MySqlPoolOptions::new()
        .max_connections(50)
        .connect_timeout(Duration::from_secs(60 * 5))
        .connect(&database_url)
        .await
        .suggestion("database urls must be in the form `mysql://username:password@host:port/database`")
        ?;

    let mut rdr = csv::Reader::from_path(
                "/Users/jesuscova/local/projects/rust/planetscale-csv-upload/crates/upload-pokemon-data/pokemon.csv",
    )?;



    let pokemons = rdr.deserialize().collect::<Result<Vec<PokemonCsv>, csv::Error>>()?;
    
    // Init Pokemon hash map for other tables
    let mut pokemon_map: HashMap<
        String,
        PokemonId,
    > = HashMap::new();

    let mut tasks = FuturesUnordered::new();

    for record in pokemons.clone().into_iter() {
        let pokemon_row: PokemonTableRow = record.clone().into();
        tasks.push(tokio::spawn(insert_pokemon(pool.clone(), pokemon_row.clone())));

        for ability in record.abilities.iter() {
            let pool = pool.clone();
            let pokemon_id = pokemon_row.id.clone();
            let ability = ability.clone();
            tasks.push(tokio::spawn(async move {
                sqlx::query!(
                r#"
                INSERT INTO abilities (
                    id, pokemon_id, ability
                ) VALUES (?, ?, ?)"#,
                PokemonId::new(),
                pokemon_id,
                ability
            ).execute(&pool).await }));
    
        }

        for egg_group in record.egg_groups.iter() {
            let pool = pool.clone();
            let pokemon_id = pokemon_row.id.clone();
            let egg_group = egg_group.clone();
            tasks.push(tokio::spawn(async move {
                sqlx::query!(
                    r#"
                    INSERT INTO egg_groups (
                        id, pokemon_id, egg_group
                    ) VALUES (?, ?, ?)"#,
                    PokemonId::new(),
                    pokemon_id,
                    egg_group
                ).execute(&pool)
                .await
            }))
        }

        for typing in record.typing.iter() {
            let pool = pool.clone();
            let pokemon_id = pokemon_row.id.clone();
            let typing = typing.clone();
            
            tasks.push(tokio::spawn(async move {
                sqlx::query!(r#"
                INSERT INTO typing (
                    id, pokemon_id, typing
                ) VALUES (?, ?, ?)"#,
                PokemonId::new(),
                pokemon_id,
                typing
                ).execute(&pool)
                .await
            }))
        }

        pokemon_map.insert(record.name, pokemon_row.id);
    }

    for pokemon in pokemons
        .into_iter()
        .filter(|pokemon| pokemon.evolves_from.is_some())
        {
            let name = pokemon.evolves_from.expect("This is cortesía rust");
            let pokemon_id = pokemon_map.get(&pokemon.name).unwrap().clone();
            let evolves_from_id = pokemon_map.get(&name).unwrap().clone();
            let pool = pool.clone();

            tasks.push(tokio::spawn(async move {

                sqlx::query!(
                    r#"
                    INSERT INTO evolutions(
                        id, pokemon_id, evolves_from
                    ) VALUES (?, ?, ?)"#,
                    PokemonId::new(),
                    pokemon_id,
                    evolves_from_id
                ).execute(&pool)
                .await
            }))
    }
    
    let pb = ProgressBar::new(tasks.len() as u64);
    while let Some(item) = tasks.next().await {
        item??;
        pb.inc(1);
    }
    pb.finish();
    
    Ok(())
}
