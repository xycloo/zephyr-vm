# Zephyr Actor Boilerplate
### Run zehpyr production instances locally.

> This is a work in progress. While it works it's not yet suited for produciton use and needs to be tested. 

## Get started

0. clone this repo.
1. Clone required packages (heytdep/rs-soroban-env, xycloo/multiuser-logging-service, xycloo/rs-ingest) in the same parent directory where the repostory was cloned. 
2. Build or install stellar core.
3. Create a release build for `zephyr/executor` and `zephyr/actor/*`
4. Setup the local postgres database. See DB setup.
5. Modify ./config/* as you wish (e.g pubnet or testnet).
6. Run the actor/sync client binary.
7. Run the actor/server execution layer.

You're now streaming close metas and spawning subprocesses that run the ZVM!

## Catchups

Catchups require direct connection to an event fetching API like Mercury and are not implemented yet.

## DB setup

db setup is quite minimal, you only need a postgres database setup. For creating tables, refer to the spec in `executor::db::exection::new_zephyr_table`. It will shortly be added to the executor binary as a helper.

**Remember to set the INGESTOR_DB env variable pointing to your postgres database**.

If you wish to serve the database thorugh an API, we recommend checking out graphile.