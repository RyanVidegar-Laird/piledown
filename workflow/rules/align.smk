rule star_align:
    input:
        r1="workflow/results/fastq/{sample}_1.fastq.gz",
        r2="workflow/results/fastq/{sample}_2.fastq.gz",
        index="workflow/results/reference/star_index",
        gtf="workflow/results/reference/gencode.v{release}.annotation.gtf.gz".format(
            release=config["gencode_release"]
        ),
    output:
        bam="workflow/results/aligned/{sample}.sorted.bam",
    log:
        "workflow/results/logs/star_align_{sample}.log",
    params:
        tmpdir="workflow/results/aligned/{sample}_star_tmp/",
    threads: config["star"]["threads"]
    shell:
        """
        STAR --runMode alignReads \
            --runThreadN {threads} \
            --readFilesIn {input.r1} {input.r2} \
            --readFilesCommand zcat \
            --genomeDir {input.index} \
            --sjdbGTFfile {input.gtf} \
            --sjdbOverhang {config[star][sjdb_overhang]} \
            --outSAMtype BAM SortedByCoordinate \
            --outSAMmode NoQS \
            --outSAMunmapped Within \
            --outFileNamePrefix {params.tmpdir} \
            2> {log}
        mv {params.tmpdir}Aligned.sortedByCoord.out.bam {output.bam}
        rm -rf {params.tmpdir}
        """


rule samtools_index:
    input:
        "workflow/results/aligned/{sample}.sorted.bam",
    output:
        "workflow/results/aligned/{sample}.sorted.bam.bai",
    log:
        "workflow/results/logs/samtools_index_{sample}.log",
    threads: 4
    shell:
        "samtools index -@ {threads} {input} 2> {log}"
