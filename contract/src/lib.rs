
// To conserve gas, efficient serialization is achieved through Borsh (http://borsh.io/)
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::{env, near_bindgen, setup_alloc, Timestamp, AccountId, Balance, Promise};
use near_sdk::collections::{UnorderedMap, Vector};
use std::time::{SystemTime, UNIX_EPOCH};


setup_alloc!();

// Structs in Rust are similar to other languages, and may include impl keyword as shown below
// Note: the names of the structs are not important when calling the smart contract, but the function names are
#[derive(BorshDeserialize, BorshSerialize)]
pub struct Entry {
    bet_owner: AccountId,
    prediction: String,
    stake: Balance
}

#[derive(BorshDeserialize, BorshSerialize)]
pub struct Pool {
    pool_id: String,
    pool_owner: AccountId,
    question: String,
    description: String,
    win_options: Vec<String>,
    close_date_time:Timestamp, //milliseconds
    result_verify_url: String,
    result: Option<String>,
    entries: Vector<Entry>,
    pool_fee: u8, // how much the pool owner wants to charge as a percentage. 
    prize_pool: u128,
    win_option_bal: UnorderedMap<String, Balance>
}

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize)]
pub struct Pools {
    owner: AccountId, 
    pools_list: UnorderedMap<AccountId, Vector<Pool>>,
    pools_blocked: bool
}

impl Default for Pools {
    fn default() -> Self {
        panic!("Should be initialized before usage")
    }
}

#[near_bindgen]
impl Pools {

    #[init]
    pub fn new() -> Self {
        assert!(env::is_valid_account_id(env::signer_account_id().as_bytes()), "Invalid owner account");
        assert!(!env::state_exists(), "Already initialized");
        Self {
            owner: env::signer_account_id(),
            pools_list: UnorderedMap::new(b'e'),
            pools_blocked: false
        }
    }
  
    pub fn make_new_pool(&mut self, question: String, desc: String, win_ops: Vec<String>, close: Timestamp, fee: u8, verify: String){
        assert!(self.pools_blocked, "Pools blocked");
        //make it so need 1 near to make pool, which also serves as the start value of the pool
        let id: String = self.pools_list.len().to_string();
        let mut win_opt_bal:UnorderedMap<String,Balance> = UnorderedMap::new(b'l');
        for o in win_ops.iter(){
            win_opt_bal.insert(o, &0);
        } 
        let pool = Pool {
            pool_id: id,
            pool_owner: env::signer_account_id(),
            question: question,
            description: desc,
            win_options: win_ops,
            close_date_time: close,
            entries: Vector::new(b'd'),
            pool_fee: fee,
            result_verify_url: verify,
            result: None,
            prize_pool: 0,
            win_option_bal: win_opt_bal
        };
        if self.pools_list.len() == 0 {
            //list is empty
            let mut pool_list = Vector::new(b'f');
            pool_list.push(&pool);
            self.pools_list.insert(&env::signer_account_id(),&pool_list);
        }else if self.pools_list.get(&env::signer_account_id()).is_none(){
            // list not emplty but first pool for this user. 
            let mut pool_list = Vector::new(b'f');
            pool_list.push(&pool);
            self.pools_list.insert(&env::signer_account_id(),&pool_list);
        }else {
            //user already has at least 1 pool in thier list. 
            assert!(self.pools_list.get(&env::signer_account_id()).is_some(), "Not found array of pools for this user");
            let mut cur_pools: Vector<Pool> = self.pools_list.get(&env::signer_account_id()).unwrap();
            cur_pools.push(&pool);
        }
    }
    #[payable]
    pub fn enter_a_pool(&mut self, owner:AccountId, pool_id: String, prediction: String, amount: Balance){
        assert!(env::attached_deposit() == amount, "No enough attached ");
        let start = SystemTime::now();
        let since_the_epoch = start
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards");
        let in_ms = since_the_epoch.as_secs() * 1000 +
            since_the_epoch.subsec_nanos() as u64 / 1_000_000;
        assert!(self.pools_list.get(&owner).is_some(), "No pools for ths owenr bad show");
        let the_pools = self.pools_list.get(&owner).unwrap();
        //create the entry 
        let entry = Entry{
            bet_owner: env::signer_account_id(),
            prediction: prediction, // make sure this is one of the possible options 
            stake: amount,
        };
        //find the correct pool to enter 
        let mut i = 0; 
        for mut p in the_pools.iter() {
            if p.pool_id == pool_id{
                //check pool time is not closed 
                assert!(p.close_date_time > in_ms, "Pool closed mate");
                // add the entry to the pool 
                p.entries.push(&entry);
                p.prize_pool = p.prize_pool + amount;
                let new_bal = p.win_option_bal.get(&entry.prediction);
                assert!(new_bal.is_some(), "No Balance wtf");
                p.win_option_bal.insert(&entry.prediction, &(new_bal.unwrap() + amount));
                // return updated pool to the list of pools. 
                self.pools_list.get(&owner).unwrap().replace(i, &p);
                
            }
            i = i + 1;
        }
    }

    pub fn add_result(&mut self, pool_id: String, result: String){
        let owner = env::signer_account_id();
        assert!(self.pools_list.get(&owner).is_some(), "You don't own any pools");
        let the_pools = self.pools_list.get(&owner).unwrap();
        let mut i = 0;
        let r = result;
        for mut p in the_pools.iter() {
            if p.pool_id == pool_id{
                let start = SystemTime::now();
                let since_the_epoch = start.duration_since(UNIX_EPOCH).expect("Time went backwards");
                let in_ms = since_the_epoch.as_secs() * 1000 + since_the_epoch.subsec_nanos() as u64 / 1_000_000;
                assert!(p.close_date_time < in_ms, "Pool not closed so not ready for result");
                p.result = Some(r.clone());
                self.pools_list.get(&owner).unwrap().replace(i, &p);
            }
            i = i + 1;
        }

    }
    
    pub fn pay_out_winners(self, pool_id:String){
        let owner = env::signer_account_id();
        assert!(self.pools_list.get(&owner).is_some(), "You don't own any pools");
        let the_pools = self.pools_list.get(&owner).unwrap();
        let mut i = 0; 
        for p in the_pools.iter(){
            //find the pool 
            if p.pool_id == pool_id {
                //loop through entires to find matches 
                assert!(p.result.is_some(), "NO result yet");
                let r = p.result.unwrap();
                for w in p.entries.iter(){
                    if w.prediction == r.clone(){
                        //winner 
                        //work out how much they get 
                        let win_bal = p.win_option_bal.get(&w.prediction);
                        let stake = w.stake;
                        assert!(win_bal.is_some(), "No win balance??");
                        let percent_due:f64 = (stake/win_bal.unwrap()) as f64;
                        let plat_fee = (p.prize_pool /100) * 3; // remove 3% platform fee
                        let owner_fee = (p.prize_pool /100) * (p.pool_fee) as u128; // remove the owner pool fee
                        let payout_total = p.prize_pool - plat_fee - owner_fee;
                        let winner_payout = payout_total as f64 * percent_due;
                        Promise::new(w.bet_owner.to_string()).transfer(winner_payout as u128);
                        //record as paid 
                    }
                }
            }
            i = i + 1
        }
    }
    pub fn payout_fees(self, pool_id:String){
        let owner = env::signer_account_id();
        assert!(self.pools_list.get(&owner).is_some(), "You don't own any pools");
    }

    pub fn block_new_pool_creation(&mut self){
        assert!(env::signer_account_id() == self.owner, "not the owner");
        self.pools_blocked = true;
    }
    pub fn unblock_new_pool_creation(&mut self){
        assert!(env::signer_account_id() == self.owner, "not the owner");
        self.pools_blocked = false;
    }
    pub fn contract_balance(self, to_acc: AccountId){
        assert!(env::signer_account_id() == self.owner, "not the owner");
        assert!(self.pools_blocked, "Pools not blocked");
        Promise::new(to_acc).transfer(env::account_balance());
        //empty balance from contract assuming no pools running.
    }

}
    // `match` is similar to `switch` in other languages; here we use it to default to "Hello" if
    // self.records.get(&account_id) is not yet defined.
    // Learn more: https://doc.rust-lang.org/book/ch06-02-match.html#matching-with-optiont

/*
 * The rest of this file holds the inline tests for the code above
 * Learn more about Rust tests: https://doc.rust-lang.org/book/ch11-01-writing-tests.html
 *
 * To run from contract directory:
 * cargo test -- --nocapture
 *
 * From project root, to run in combination with frontend tests:
 * yarn test
 *
 */
#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::MockedBlockchain;
    use near_sdk::{testing_env, VMContext};

    // mock the context for testing, notice "signer_account_id" that was accessed above from env::
    fn get_context(input: Vec<u8>, is_view: bool) -> VMContext {
        VMContext {
            current_account_id: "alice_near".to_string(),
            signer_account_id: "bob_near".to_string(),
            signer_account_pk: vec![0, 1, 2],
            predecessor_account_id: "carol_near".to_string(),
            input,
            block_index: 0,
            block_timestamp: 0,
            account_balance: 0,
            account_locked_balance: 0,
            storage_usage: 0,
            attached_deposit: 0,
            prepaid_gas: 10u64.pow(18),
            random_seed: vec![0, 1, 2],
            is_view,
            output_data_receivers: vec![],
            epoch_height: 19,
        }
    }

    #[test]
    fn make_new_pool(){
        let start = SystemTime::now();
        let since_the_epoch = start
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards");
        let in_ms = since_the_epoch.as_secs() * 1000 +
            since_the_epoch.subsec_nanos() as u64 / 1_000_000;

        println!("Time now is {:?}", in_ms);
        let context = get_context(vec![], false);
        testing_env!(context);
        let mut contract = Pools::new();
        let win_options: Vec<String> = vec!["yes".to_string(), "no".to_string()];
        contract.make_new_pool("will it rain?".to_string(), 
        "Will we see rain today?".to_string(),win_options , 16358668940, 2, "Some Url".to_string());
        println!("Made  a new pool");
        println!("{:?}",contract.pools_list.len());
    }
}

//     #[test]
//     fn set_then_get_greeting() {
//         let context = get_context(vec![], false);
//         testing_env!(context);
//         let mut contract = Welcome::default();
//         contract.set_greeting("howdy".to_string());
//         assert_eq!(
//             "howdy".to_string(),
//             contract.get_greeting("bob_near".to_string())
//         );
//     }

//     #[test]
//     fn get_default_greeting() {
//         let context = get_context(vec![], true);
//         testing_env!(context);
//         let contract = Welcome::default();
//         // this test did not call set_greeting so should return the default "Hello" greeting
//         assert_eq!(
//             "Hello".to_string(),
//             contract.get_greeting("francis.near".to_string())
//         );
//     }
 
