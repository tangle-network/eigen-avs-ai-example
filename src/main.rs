use blueprint::{TaskManager, TASK_MANAGER_ADDRESS};
use blueprint_sdk::alloy::primitives::Address;
use blueprint_sdk::logging::info;
use blueprint_sdk::macros::main;
use blueprint_sdk::runners::core::runner::BlueprintRunner;
use blueprint_sdk::runners::eigenlayer::bls::EigenlayerBLSConfig;
use blueprint_sdk::utils::evm::get_provider_http;
use eigen_ai_avs_example as blueprint;
use eigen_ai_avs_example::jobs::github::GithubSpellingJobEventHandler;

#[main(env)]
async fn main() {
    // Create your service context
    // Here you can pass any configuration or context that your service needs.
    let context = blueprint::ExampleContext {
        config: env.clone(),
    };

    // Get the provider
    let rpc_endpoint = env.http_rpc_endpoint.clone();
    let provider = get_provider_http(&rpc_endpoint);

    // Create an instance of your task manager
    let contract = TaskManager::new(*TASK_MANAGER_ADDRESS, provider);

    // Create the event handler from the job
    let say_hello_job = blueprint::SayHelloEventHandler::new(contract.clone(), context.clone());
    let github_spelling_job = GithubSpellingJobEventHandler::new(contract, context.clone());

    info!("Starting the event watcher ...");
    let eigen_config = EigenlayerBLSConfig::new(Address::default(), Address::default());
    BlueprintRunner::new(eigen_config, env)
        .job(say_hello_job)
        .job(github_spelling_job)
        .run()
        .await?;

    info!("Exiting...");
    Ok(())
}
