use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

declare_id!("4JD6CXFyXdn2z5trTvcMm42yMygUmkdnbvJmPSd98KbQ");

#[program]
pub mod solvent_labs {
    use super::*;

    pub fn initialize(
        ctx: Context<Initialize>,
        company_percentage: u8,
        pool_percentage: u8,
    ) -> Result<()> {
        require!(company_percentage <= 100, CustomError::InvalidFeePercentage);

        let state = &mut ctx.accounts.state;
        state.owner = ctx.accounts.owner.key();
        state.duel_count = 0;
        state.company_percentage = company_percentage;
        state.pool_percentage = pool_percentage;
        state.company = ctx.accounts.company.key();
        state.pool = ctx.accounts.pool.key();
        state.token_mint = ctx.accounts.token_mint.key();
        state.vault_bump = ctx.bumps.vault; // Direct access to bump

        Ok(())
    }

    pub fn create_duel(ctx: Context<CreateDuel>, wager_amount: u64) -> Result<()> {
        require!(wager_amount > 0, CustomError::MinimumBetRequired);

        // Check if host has enough tokens
        require!(
            ctx.accounts.host_token_account.amount >= wager_amount,
            CustomError::InsufficientFunds
        );

        let state = &mut ctx.accounts.state;
        state.duel_count += 1;

        let duel = &mut ctx.accounts.duel;
        duel.id = state.duel_count;
        duel.host = ctx.accounts.host.key();
        duel.joiner = Pubkey::default();
        duel.wager_amount = wager_amount;
        duel.status = DuelStatus::Pending;
        duel.winner = Pubkey::default();
        duel.start_time = Clock::get()?.unix_timestamp as u64;
        duel.bump = ctx.bumps.duel;

        emit!(DuelCreated {
            duel_id: duel.id,
            owner: duel.host,
        });

        Ok(())
    }

    pub fn join_duel(ctx: Context<JoinDuel>) -> Result<()> {
        let duel = &mut ctx.accounts.duel;

        require!(duel.status == DuelStatus::Pending, CustomError::DuelNotOpen);
        require!(
            ctx.accounts.joiner.key() != duel.host,
            CustomError::CannotPlayAgainstSelf
        );

        // Check if joiner has enough tokens
        require!(
            ctx.accounts.joiner_token_account.amount >= duel.wager_amount,
            CustomError::InsufficientFunds
        );

        duel.status = DuelStatus::Joined;
        duel.joiner = ctx.accounts.joiner.key();

        emit!(DuelJoined {
            duel_id: duel.id,
            owner: duel.joiner,
        });

        Ok(())
    }

    pub fn start_duel(ctx: Context<StartDuel>) -> Result<()> {
        let duel = &mut ctx.accounts.duel;
        require!(duel.status == DuelStatus::Joined, CustomError::DuelNotReady);

        // Get vault signer seeds
        let vault_seeds = &[b"vault".as_ref(), &[ctx.accounts.state.vault_bump]];
        let vault_signer = &[&vault_seeds[..]];
        // Transfer tokens from host to vault
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.host_token_account.to_account_info(),
                    to: ctx.accounts.vault_token_account.to_account_info(),
                    authority: ctx.accounts.vault.to_account_info(),
                },
                vault_signer,
            ),
            duel.wager_amount,
        )?;

        // Transfer tokens from joiner to vault
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.joiner_token_account.to_account_info(),
                    to: ctx.accounts.vault_token_account.to_account_info(),
                    authority: ctx.accounts.vault.to_account_info(),
                },
                vault_signer,
            ),
            duel.wager_amount,
        )?;

        duel.status = DuelStatus::Active;

        emit!(DuelStarted {
            duel_id: duel.id,
            owner: duel.host,
        });

        Ok(())
    }

    pub fn distribute_rewards(ctx: Context<DistributeRewards>, winner: Pubkey) -> Result<()> {
        let duel = &mut ctx.accounts.duel;
        let state = &ctx.accounts.state;

        require!(
            duel.status == DuelStatus::Active,
            CustomError::DuelNotActive
        );
        require!(
            winner == duel.host || winner == duel.joiner,
            CustomError::InvalidWinner
        );

        let total_wager = duel.wager_amount.checked_mul(2).unwrap();
        let company_amount = (total_wager as u128)
            .checked_mul(state.company_percentage as u128)
            .and_then(|amt| amt.checked_div(100))
            .unwrap() as u64;
        let pool_amount = (total_wager as u128)
            .checked_mul(state.pool_percentage as u128)
            .and_then(|amt| amt.checked_div(100))
            .unwrap() as u64;
        let reward_amount = total_wager
            .checked_sub(company_amount)
            .and_then(|amt| amt.checked_sub(pool_amount))
            .unwrap();

        let vault_seeds = &[b"vault".as_ref(), &[state.vault_bump]];
        let vault_signer = &[&vault_seeds[..]];

        // Transfer reward to winner
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.vault_token_account.to_account_info(),
                    to: ctx.accounts.winner_token_account.to_account_info(),
                    authority: ctx.accounts.vault.to_account_info(),
                },
                vault_signer,
            ),
            reward_amount,
        )?;

        // Transfer fee to company
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.vault_token_account.to_account_info(),
                    to: ctx.accounts.company_token_account.to_account_info(),
                    authority: ctx.accounts.vault.to_account_info(),
                },
                vault_signer,
            ),
            company_amount,
        )?;

        // Transfer fee to pool
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.vault_token_account.to_account_info(),
                    to: ctx.accounts.pool_token_account.to_account_info(),
                    authority: ctx.accounts.vault.to_account_info(),
                },
                vault_signer,
            ),
            pool_amount,
        )?;

        duel.status = DuelStatus::Completed;
        duel.winner = winner;

        emit!(DuelDecided {
            winner_id: winner,
            duel_id: duel.id,
        });

        Ok(())
    }

    pub fn update_company_allocation(
        ctx: Context<UpdateCompanyAllocation>,
        new_company_percentage: u8,
    ) -> Result<()> {
        require!(
            new_company_percentage > 0 && new_company_percentage <= 100,
            CustomError::InvalidFeePercentage
        );

        let state = &mut ctx.accounts.state;
        let old_company_percentage = state.company_percentage;
        state.company_percentage = new_company_percentage;

        emit!(CompanyAllocationUpdated {
            old_company_percentage,
            new_company_percentage,
        });

        Ok(())
    }

    pub fn update_pool_allocation(
        ctx: Context<UpdatePoolAllocation>,
        new_pool_percentage: u8,
    ) -> Result<()> {
        require!(
            new_pool_percentage > 0 && new_pool_percentage <= 100,
            CustomError::InvalidFeePercentage
        );

        let state = &mut ctx.accounts.state;
        let old_pool_percentage = state.pool_percentage;
        state.pool_percentage = new_pool_percentage;

        emit!(PoolAllocationUpdated {
            old_pool_percentage,
            new_pool_percentage,
        });

        Ok(())
    }

    pub fn update_company_address(
        ctx: Context<UpdateCompanyAddress>,
        new_company_address: Pubkey,
    ) -> Result<()> {
        require!(
            new_company_address != Pubkey::default(),
            CustomError::InvalidAddress
        );

        let state = &mut ctx.accounts.state;
        let old_company_address = state.company;
        state.company = new_company_address;

        emit!(CompanyAddressUpdated {
            old_company_address: old_company_address,
            new_company_address: new_company_address,
        });

        Ok(())
    }

    pub fn update_pool_address(
        ctx: Context<UpdatePoolAddress>,
        new_pool_address: Pubkey,
    ) -> Result<()> {
        require!(
            new_pool_address != Pubkey::default(),
            CustomError::InvalidAddress
        );

        let state = &mut ctx.accounts.state;
        let old_pool_address = state.pool;
        state.pool = new_pool_address;

        emit!(PoolAddressUpdated {
            old_pool_address: old_pool_address,
            new_pool_address: new_pool_address,
        });

        Ok(())
    }

    pub fn update_token_mint(ctx: Context<UpdateTokenMint>, new_token_mint: Pubkey) -> Result<()> {
        require!(
            new_token_mint != Pubkey::default(),
            CustomError::InvalidAddress
        );

        let state = &mut ctx.accounts.state;
        let old_token_mint = state.token_mint;
        state.token_mint = new_token_mint;

        emit!(TokenMintUpdated {
            old_token_mint,
            new_token_mint,
        });

        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(init, payer = owner, space = 8 + State::SPACE)]
    pub state: Account<'info, State>,
    #[account(mut)]
    pub owner: Signer<'info>,
    /// CHECK: Company account
    pub company: AccountInfo<'info>,
    /// CHECK: pool account
    pub pool: AccountInfo<'info>,
    pub token_mint: Account<'info, Mint>,
    /// CHECK: PDA for vault
    #[account(
        init,
        payer = owner,
        space = 8,  // minimum space for the account
        seeds = [b"vault"],
        bump
    )]
    pub vault: AccountInfo<'info>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
#[instruction()]
pub struct CreateDuel<'info> {
    #[account(mut)]
    pub state: Account<'info, State>,
    #[account(
        init,
        payer = owner,
        space = 8 + Duel::SPACE,
        seeds = [
            b"duel",
            state.duel_count.to_le_bytes().as_ref(),
        ],
        bump
    )]
    pub duel: Account<'info, Duel>,
    #[account(mut)]
    pub owner: Signer<'info>,
    /// CHECK: Host wallet address
    #[account(mut)]
    pub host: AccountInfo<'info>,
    #[account(
        mut,
        associated_token::mint = state.token_mint,
        associated_token::authority = host
    )]
    pub host_token_account: Box<Account<'info, TokenAccount>>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct JoinDuel<'info> {
    pub state: Account<'info, State>,
    #[account(mut)]
    pub duel: Account<'info, Duel>,
    /// CHECK: Joiner wallet address
    pub joiner: AccountInfo<'info>,
    #[account(
        mut,
        associated_token::mint = state.token_mint,
        associated_token::authority = joiner
    )]
    pub joiner_token_account: Box<Account<'info, TokenAccount>>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

#[derive(Accounts)]
pub struct StartDuel<'info> {
    pub state: Account<'info, State>,
    #[account(mut)]
    pub owner: Signer<'info>,
    #[account(mut)]
    pub duel: Account<'info, Duel>,
    /// CHECK: Host wallet address
    pub host: AccountInfo<'info>,
    /// CHECK: Joiner wallet address
    pub joiner: AccountInfo<'info>,
    #[account(
        constraint = token_mint.key() == state.token_mint
    )]
    pub token_mint: Account<'info, Mint>,
    #[account(
        mut,
        associated_token::mint = token_mint,
        associated_token::authority = host
    )]
    pub host_token_account: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        associated_token::mint = token_mint,
        associated_token::authority = joiner
    )]
    pub joiner_token_account: Box<Account<'info, TokenAccount>>,
    #[account(
        init_if_needed,
        payer = owner,
        associated_token::mint = token_mint,
        associated_token::authority = vault
    )]
    pub vault_token_account: Box<Account<'info, TokenAccount>>,
    /// CHECK: PDA for vault
    #[account(
        seeds = [b"vault"],
        bump = state.vault_bump
    )]
    pub vault: AccountInfo<'info>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

#[derive(Accounts)]
pub struct DistributeRewards<'info> {
    #[account(mut, has_one = owner)]
    pub state: Account<'info, State>,
    #[account(mut)]
    pub owner: Signer<'info>,
    #[account(mut)]
    pub duel: Account<'info, Duel>,
    #[account(
        constraint = token_mint.key() == state.token_mint
    )]
    pub token_mint: Account<'info, Mint>,
    #[account(
        mut,
        associated_token::mint = token_mint,
        associated_token::authority = vault
    )]
    pub vault_token_account: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        associated_token::mint = token_mint,
        associated_token::authority = winner
    )]
    pub winner_token_account: Box<Account<'info, TokenAccount>>,
    #[account(
        init_if_needed,
        payer = owner,
        associated_token::mint = token_mint,
        associated_token::authority = company
    )]
    pub company_token_account: Box<Account<'info, TokenAccount>>,
    #[account(
        init_if_needed,
        payer = owner,
        associated_token::mint = token_mint,
        associated_token::authority = pool
    )]
    pub pool_token_account: Box<Account<'info, TokenAccount>>,
    /// CHECK: Winner wallet address
    #[account(constraint = (winner.key() == duel.host || winner.key() == duel.joiner))]
    pub winner: AccountInfo<'info>,
    /// CHECK: Company wallet address
    #[account(constraint = company.key() == state.company)]
    pub company: AccountInfo<'info>,
    /// CHECK: Pool wallet address
    #[account(constraint = pool.key() == state.pool)]
    pub pool: AccountInfo<'info>,
    /// CHECK: PDA for vault
    #[account(
        seeds = [b"vault"],
        bump = state.vault_bump
    )]
    pub vault: AccountInfo<'info>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

#[derive(Accounts)]
pub struct UpdateCompanyAllocation<'info> {
    #[account(mut, has_one = owner)]
    pub state: Account<'info, State>,
    pub owner: Signer<'info>,
}

#[derive(Accounts)]
pub struct UpdatePoolAllocation<'info> {
    #[account(mut, has_one = owner)]
    pub state: Account<'info, State>,
    pub owner: Signer<'info>,
}

#[derive(Accounts)]
pub struct UpdateCompanyAddress<'info> {
    #[account(mut, has_one = owner)]
    pub state: Account<'info, State>,
    pub owner: Signer<'info>,
}

#[derive(Accounts)]
pub struct UpdatePoolAddress<'info> {
    #[account(mut, has_one = owner)]
    pub state: Account<'info, State>,
    pub owner: Signer<'info>,
}

// Account structures
#[account]
pub struct State {
    pub owner: Pubkey,
    pub duel_count: u64,
    pub company_percentage: u8,
    pub pool_percentage: u8,
    pub company: Pubkey,
    pub pool: Pubkey,
    pub token_mint: Pubkey,
    pub vault_bump: u8,
}

#[account]
pub struct Duel {
    pub id: u64,
    pub host: Pubkey,
    pub joiner: Pubkey,
    pub wager_amount: u64,
    pub status: DuelStatus,
    pub winner: Pubkey,
    pub start_time: u64,
    pub bump: u8,
}

// Enums and Constants
#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq)]
pub enum DuelStatus {
    Pending,
    Joined,
    Active,
    Completed,
}

// Events
#[event]
pub struct DuelCreated {
    pub duel_id: u64,
    pub owner: Pubkey,
}

#[event]
pub struct DuelJoined {
    pub duel_id: u64,
    pub owner: Pubkey,
}

#[event]
pub struct DuelStarted {
    pub duel_id: u64,
    pub owner: Pubkey,
}

#[event]
pub struct DuelDecided {
    pub duel_id: u64,
    pub winner_id: Pubkey,
}

#[event]
pub struct CompanyAllocationUpdated {
    pub old_company_percentage: u8,
    pub new_company_percentage: u8,
}

#[event]
pub struct PoolAllocationUpdated {
    pub old_pool_percentage: u8,
    pub new_pool_percentage: u8,
}

#[event]
pub struct CompanyAddressUpdated {
    pub old_company_address: Pubkey,
    pub new_company_address: Pubkey,
}

#[event]
pub struct PoolAddressUpdated {
    pub old_pool_address: Pubkey,
    pub new_pool_address: Pubkey,
}

// Error codes
#[error_code]
pub enum CustomError {
    #[msg("A minimum bet is required")]
    MinimumBetRequired,
    #[msg("Insufficient funds")]
    InsufficientFunds,
    #[msg("Duel is not open to join")]
    DuelNotOpen,
    #[msg("You can't play against yourself")]
    CannotPlayAgainstSelf,
    #[msg("Duel is not ready to start")]
    DuelNotReady,
    #[msg("Duel is not active")]
    DuelNotActive,
    #[msg("Invalid winner address")]
    InvalidWinner,
    #[msg("Invalid fee percentage")]
    InvalidFeePercentage,
    #[msg("Invalid address")]
    InvalidAddress,
}

#[derive(Accounts)]
pub struct UpdateTokenMint<'info> {
    #[account(mut, has_one = owner)]
    pub state: Account<'info, State>,
    pub owner: Signer<'info>,
    pub new_token_mint: Account<'info, Mint>,
}

#[event]
pub struct TokenMintUpdated {
    pub old_token_mint: Pubkey,
    pub new_token_mint: Pubkey,
}

// Helper implementations
impl State {
    pub const SPACE: usize = 32 + // owner
        8 + // duel_count
        1 + // company_percentage
        1 + // pool_percentage
        32 + // company
        32 + // pool
        32 + // token_mint
        1; // vault_bump
}

impl Duel {
    pub const SPACE: usize = 8 + // id
        32 + // host
        32 + // joiner
        8 + // wager_amount
        1 + // status
        32 + // winner
        8 + // start_time
        1; // bump
}
